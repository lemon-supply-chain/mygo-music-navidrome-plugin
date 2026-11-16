#!/usr/bin/env bash
# End-to-end check: save two servers, switch, edit, remove, and hand the one in
# use to the callback the plugin would be listening on.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
crate="$(dirname "$here")"
work="$(mktemp -d)"
binary="$crate/target/debug/navidrome-login"

failures=0
check() {
  if [ "$2" != "$3" ]; then
    echo "FAIL $1: expected [$3], got [$2]"
    failures=$((failures + 1))
  else
    echo "ok   $1"
  fi
}

contains() {
  if grep -qF "$2" "$3"; then echo "ok   $1"; else echo "FAIL $1: [$2] not in $3"; failures=$((failures + 1)); fi
}

python3 "$here/fake-navidrome.py" alice "secret pass" 4545 &
python3 "$here/fake-navidrome.py" bob "hunter2" 4546 &
mocks=$!

callback_file="$work/callback"
echo "mocks: $mocks"
sleep 1

"$binary" --dir "$work" --port 4599 >"$work/out.log" 2>"$work/err.log" &
app=$!
for _ in $(seq 40); do
  [ -s "$work/out.log" ] && break
  sleep 0.1
done

ready="$(head -1 "$work/out.log")"
check "ready line is one object" "$(echo "$ready" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["type"])')" "ready"
url="$(echo "$ready" | python3 -c 'import json,sys; print(json.load(sys.stdin)["url"])')"

# The helper says where Done will send the account; the plugin listens there.
callback="$(echo "$ready" | python3 -c 'import json,sys; print(json.load(sys.stdin)["callback"])')"
check "ready carries the callback" "$(python3 -c 'import sys,urllib.parse as u; print(u.urlparse(sys.argv[1]).path)' "$callback")" "/navidrome"
check "the callback port is not the page's" "$(test "$(python3 -c 'import sys,urllib.parse as u; print(u.urlparse(sys.argv[1]).port)' "$callback")" != "4599" && echo apart)" "apart"
callback_port="$(python3 -c 'import sys,urllib.parse as u; print(u.urlparse(sys.argv[1]).port)' "$callback")"
python3 -c '
import http.server, pathlib, sys
out = pathlib.Path(sys.argv[1])
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        out.write_text(self.path)
        body = b"ok"
        self.send_response(200)
        self.send_header("content-type", "text/plain")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *_):
        pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[2])), Handler).serve_forever()
' "$callback_file" "$callback_port" &
callback_app=$!
sleep 0.3

jar="$work/jar"
check "page needs the token" "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/)" "403"
check "bootstrap redirect sets a cookie" "$(curl -s -o /dev/null -w '%{http_code}' -c "$jar" "$url")" "303"
check "page loads" "$(curl -s -b "$jar" -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/)" "200"
check "css loads" "$(curl -s -b "$jar" -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/app.css)" "200"
check "js loads" "$(curl -s -b "$jar" -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/app.js)" "200"

page="$work/page.html"
curl -s -b "$jar" -o "$page" http://127.0.0.1:4599/
contains "page ships the add button" 'id="add"' "$page"
contains "the form lives in a dialog" '<dialog class="modal" id="modal"' "$page"
contains "the page has a way to finish" 'class="finish" id="done"' "$page"

post() { curl -s -b "$jar" -H 'content-type: application/json' -d "$2" "http://127.0.0.1:4599$1"; }

wrong="$(post /api/login '{"url":"http://127.0.0.1:4545","username":"alice","password":"nope"}')"
check "wrong password is refused" "$(echo "$wrong" | python3 -c 'import json,sys; print(json.load(sys.stdin)["error"]["kind"])')" "credentials"
check "nothing is handed back before signing in" "$(curl -s -b "$jar" -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/done)" "303"

first="$(post /api/login '{"name":"Living room","url":"http://127.0.0.1:4545/rest/","username":"alice","password":"secret pass"}')"
check "first account is signed in" "$(echo "$first" | python3 -c 'import json,sys; print(json.load(sys.stdin)["account"]["label"])')" "Living room"
check "the server is normalized" "$(echo "$first" | python3 -c 'import json,sys; print(json.load(sys.stdin)["account"]["url"])')" "http://127.0.0.1:4545"
check "the answer keeps the password on the server" "$(echo "$first" | grep -c '"password"' || true)" "0"
first_id="$(echo "$first" | python3 -c 'import json,sys; print(json.load(sys.stdin)["account"]["id"])')"

check "auth file exists" "$(test -f "$work/auth.json" && echo yes)" "yes"
check "auth file is private" "$(stat -f '%Lp' "$work/auth.json")" "600"

second="$(post /api/login '{"url":"127.0.0.1:4546","username":"bob","password":"hunter2"}')"
check "second account is signed in" "$(echo "$second" | python3 -c 'import json,sys; print(json.load(sys.stdin)["account"]["url"])')" "http://127.0.0.1:4546"
second_id="$(echo "$second" | python3 -c 'import json,sys; print(json.load(sys.stdin)["account"]["id"])')"

state="$(curl -s -b "$jar" http://127.0.0.1:4599/api/state)"
check "both accounts are listed" "$(echo "$state" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["accounts"]))')" "2"
check "the newest is current" "$(echo "$state" | python3 -c 'import json,sys; print(json.load(sys.stdin)["current"])')" "$second_id"
check "passwords stay on the server" "$(echo "$state" | grep -c '"password"' || true)" "0"

edited="$(curl -s -b "$jar" -X PUT -H 'content-type: application/json' \
  -d '{"name":"Renamed","url":"http://127.0.0.1:4545/","username":"alice","password":""}' \
  "http://127.0.0.1:4599/api/accounts/$first_id")"
check "editing rewrites the account" "$(echo "$edited" | python3 -c 'import json,sys; d=json.load(sys.stdin)["account"]; print(d["label"], d["id"], d["url"])')" \
  "Renamed $first_id http://127.0.0.1:4545"
check "a blank password keeps the saved one" "$(python3 -c "import json;print(json.load(open('$work/auth.json'))['accounts'][0]['password'])")" "secret pass"

check "a wrong password is refused on edit" "$(curl -s -b "$jar" -X PUT -H 'content-type: application/json' -d '{"url":"http://127.0.0.1:4546","username":"bob","password":"nope"}' -o /dev/null -w '%{http_code}' "http://127.0.0.1:4599/api/accounts/$second_id")" "400"
check "the same server and user cannot clash" "$(curl -s -b "$jar" -X PUT -H 'content-type: application/json' -d '{"url":"http://127.0.0.1:4545","username":"alice","password":"secret pass"}' -o /dev/null -w '%{http_code}' "http://127.0.0.1:4599/api/accounts/$second_id")" "409"

post /api/switch "{\"id\":\"$first_id\"}" >/dev/null
state="$(curl -s -b "$jar" http://127.0.0.1:4599/api/state)"
check "switching moves the current account" "$(echo "$state" | python3 -c 'import json,sys; print(json.load(sys.stdin)["current"])')" "$first_id"

done="$work/done.html"
check "done shows a card" "$(curl -s -b "$jar" -o "$done" -w '%{http_code}' http://127.0.0.1:4599/done)" "200"
contains "the card says it is signed in" 'id="title">Signed in' "$done"
contains "the card names the server in use" '>Renamed<' "$done"
contains "the card tries to close itself" "window.close()" "$done"
contains "the card waits a second" "}, 1000);" "$done"

for _ in $(seq 50); do
  [ -s "$callback_file" ] && break
  sleep 0.1
done
check "the callback was answered" "$(test -s "$callback_file" && echo yes || echo no)" "yes"

payload="$work/payload.json"
python3 -c '
import base64, sys
path = open(sys.argv[1]).read()
query = path.split("?", 1)[1]
data = dict(part.split("=", 1) for part in query.split("&"))["data"]
print(base64.urlsafe_b64decode(data + "=" * (-len(data) % 4)).decode("utf-8"))
' "$callback_file" >"$payload"
check "the payload is the account in use" "$(python3 -c "import json; d=json.load(open('$payload')); print(d['id'], d['name'], d['url'], d['username'], d['password'])")" \
  "$first_id Renamed http://127.0.0.1:4545 alice secret pass"

check "remove answers" "$(curl -s -b "$jar" -X DELETE -o /dev/null -w '%{http_code}' "http://127.0.0.1:4599/api/accounts/$second_id")" "200"
check "remove drops the row" "$(curl -s -b "$jar" http://127.0.0.1:4599/api/state | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["accounts"]))')" "1"
check "done leaves the page serving" "$(curl -s -b "$jar" -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/)" "200"

# Removing the last server leaves nothing to hand back.
curl -s -b "$jar" -X DELETE -o /dev/null "http://127.0.0.1:4599/api/accounts/$first_id"
check "no server is left" "$(curl -s -b "$jar" http://127.0.0.1:4599/api/state | python3 -c 'import json,sys; print(len(json.load(sys.stdin)["accounts"]))')" "0"
check "done has nothing to show" "$(curl -s -b "$jar" -o /dev/null -w '%{http_code}' http://127.0.0.1:4599/done)" "303"

kill "$app" "$callback_app" "$mocks" 2>/dev/null || true
pkill -f fake-navidrome.py 2>/dev/null || true
rm -rf "$work"
echo
[ "$failures" -eq 0 ] && echo "all checks passed" || { echo "$failures check(s) failed"; exit 1; }