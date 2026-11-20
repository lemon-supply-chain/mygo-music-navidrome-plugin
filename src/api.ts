import { API_VERSION, CLIENT_NAME, MEDIA_SALT } from "./constants";
import { current } from "./session";
import type { ArtistRef, Session } from "./types";
import { fail } from "./utils";

export const tr = (text: string) => host.t(text);

export function session(): Session {
  const value = current();
  if (!value) throw fail("signIn", tr("Sign in to your Navidrome server."));
  return value;
}

export function username() {
  return session().username;
}

export function password() {
  return session().password;
}

export function server() {
  const base = session().url.replace(/\/+$/, "");
  return /^https?:\/\//i.test(base) ? base : "http://" + base;
}

export function authPairs(): [string, string][] {
  const salt = Math.random().toString(36).slice(2, 10);
  return [
    ["u", username()],
    ["t", host.crypto.md5(password() + salt)],
    ["s", salt],
    ["v", API_VERSION],
    ["c", CLIENT_NAME],
  ];
}

export function mediaAuthPairs(): [string, string][] {
  return [
    ["u", username()],
    ["t", host.crypto.md5(password() + MEDIA_SALT)],
    ["s", MEDIA_SALT],
    ["v", API_VERSION],
    ["c", CLIENT_NAME],
  ];
}

export function pairsOf(params: Record<string, unknown>): [string, string][] {
  const pairs: [string, string][] = [];
  for (const key in params) {
    const value = params[key];
    if (value === undefined || value === null) continue;
    if (Array.isArray(value)) {
      for (const item of value) pairs.push([key, String(item)]);
    } else {
      pairs.push([key, String(value)]);
    }
  }
  return pairs;
}

export function urlFor(path: string, pairs: [string, string][]) {
  const query = pairs.map((pair) => encodeURIComponent(pair[0]) + "=" + encodeURIComponent(pair[1])).join("&");
  return server() + path + "?" + query;
}

export function endpoint(method: string, params?: Record<string, unknown>) {
  return urlFor("/rest/" + method + ".view", authPairs().concat(pairsOf(params || {}), [["f", "json"]]));
}

export function mediaURL(method: string, params?: Record<string, unknown>) {
  return urlFor("/rest/" + method + ".view", mediaAuthPairs().concat(pairsOf(params || {})));
}

export async function nativeLogin() {
  const current = session();
  let response: Response;
  try {
    response = await fetch(server() + "/auth/login", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username: current.username, password: current.password }),
    });
  } catch (error: any) {
    throw fail("unavailable", tr("Can't reach your Navidrome server.") + " " + error.message);
  }
  if (response.status === 401 || response.status === 403) {
    throw fail("signIn", tr("Navidrome refused the username or password."));
  }
  if (!response.ok) {
    throw fail("unavailable", tr("Navidrome answered with an error.") + " (" + response.status + ")");
  }
  return response.json();
}

export async function nativeList(token: string, path: string, params?: Record<string, unknown>): Promise<any[]> {
  let response: Response;
  try {
    response = await fetch(urlFor(path, pairsOf(params || {})), {
      headers: { accept: "application/json", "X-ND-Authorization": "Bearer " + token },
    });
  } catch (error: any) {
    throw fail("unavailable", tr("Can't reach your Navidrome server.") + " " + error.message);
  }
  if (response.status === 401 || response.status === 403) {
    throw fail("signIn", tr("Navidrome refused the username or password."));
  }
  if (!response.ok) {
    throw fail("unavailable", tr("Navidrome answered with an error.") + " (" + response.status + ")");
  }
  return response.json();
}

export async function call(method: string, params?: Record<string, unknown>) {
  let response: Response;
  try {
    response = await fetch(endpoint(method, params));
  } catch (error: any) {
    throw fail("unavailable", tr("Can't reach your Navidrome server.") + " " + error.message);
  }
  if (!response.ok) {
    throw fail("unavailable", tr("Navidrome answered with an error.") + " (" + response.status + ")");
  }
  const body = await response.json();
  const data = body["subsonic-response"];
  if (!data) throw fail("unavailable", tr("Navidrome answered something that isn't a Subsonic response."));
  if (data.status === "failed") {
    const error = data.error || {};
    if (error.code === 40 || error.code === 41) {
      throw fail("signIn", tr("Navidrome refused the username or password."));
    }
    if (error.code === 70) throw fail("unavailable", tr("Navidrome couldn't find that."));
    throw new Error(error.message || tr("Navidrome refused the request."));
  }
  return data;
}

export const pack = (kind: string, id: string) => kind + ":" + id;

export function unpack(id: string) {
  const text = String(id == null ? "" : id);
  const at = text.indexOf(":");
  if (at === -1) return { kind: "tr", id: text };
  return { kind: text.slice(0, at), id: text.slice(at + 1) };
}

export const rawId = (id: string) => unpack(id).id;

export function coverURL(id?: string) {
  if (!id) return;
  return mediaURL("getCoverArt", { id, size: "{size}" });
}

export function artistRef(id?: string, name?: string): ArtistRef | null {
  if (!name) return null;
  return { id: pack("ar", id || name), name };
}
