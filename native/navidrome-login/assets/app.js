// Page logic for the local Navidrome sign-in. Talks only to the endpoints in web.rs.

const STRINGS = {
  en: {
    title: "Navidrome",
    subtitle: "Pick a saved server, or add another one.",
    saved: "Saved servers",
    add: "Add server",
    signin: "Sign in to a server",
    editTitle: "Edit server",
    edit: "Edit",
    editNamed: "Edit {name}",
    save: "Save",
    keepPassword: "Keep password",
    edited: "Saved {name}.",
    duplicate: "Another saved server already uses that address and account.",
    serverName: "Server name",
    optional: "optional",
    url: "Server URL",
    username: "Username",
    password: "Password",
    connect: "Connect",
    show: "Show",
    hide: "Hide",
    cancel: "Cancel",
    close: "Close",
    empty: "No server saved yet.",
    stored: "Credentials are kept in auth.json on this device only.",
    done: "Done",
    connecting: "Checking the server\u2026",
    switching: "Switching\u2026",
    signedIn: "Signed in as {name}.",
    switched: "Now using {name}.",
    hint: "Done hands this server to MyGo Music.",
    signingOut: "Handing it to MyGo Music\u2026",
    missing: "Fill in the server URL, username and password.",
    removeConfirm: "Remove {name} from this device?",
    removed: "Removed {name}.",
    network: "Couldn't reach that server. Check the URL, and that Navidrome is running.",
    credentials: "Wrong username or password.",
    server: "The server refused the sign-in: {message}",
    unknown: "Something went wrong: {message}",
  },
  zh: {
    title: "Navidrome",
    subtitle: "选择一个已保存的服务器，或添加另一个。",
    saved: "已保存的服务器",
    add: "添加服务器",
    signin: "登录服务器",
    editTitle: "编辑服务器",
    edit: "编辑",
    editNamed: "编辑 {name}",
    save: "保存",
    keepPassword: "保留原密码",
    edited: "已保存 {name}。",
    duplicate: "已有服务器使用了相同的地址和账号。",
    serverName: "服务器名称",
    optional: "可选",
    url: "服务器地址",
    username: "账号",
    password: "密码",
    connect: "登录",
    show: "显示",
    hide: "隐藏",
    cancel: "取消",
    close: "关闭",
    empty: "还没有保存的服务器。",
    stored: "登录信息只保存在本机的 auth.json 中。",
    done: "完成",
    connecting: "正在检查服务器\u2026",
    switching: "正在切换\u2026",
    signedIn: "已登录 {name}。",
    switched: "已切换到 {name}。",
    hint: "点击完成后，把这个服务器交给 MyGo Music。",
    signingOut: "正在交给 MyGo Music\u2026",
    missing: "请填写服务器地址、账号和密码。",
    removeConfirm: "从本机移除 {name}？",
    removed: "已移除 {name}。",
    network: "无法连接该服务器。请检查地址，以及 Navidrome 是否在运行。",
    credentials: "账号或密码不正确。",
    server: "服务器拒绝了登录：{message}",
    unknown: "出了点问题：{message}",
  },
};

const language = (new URLSearchParams(location.search).get("lang") || navigator.language || "en")
  .toLowerCase()
  .startsWith("zh")
  ? "zh"
  : "en";

function t(key, values) {
  const template = STRINGS[language][key] ?? STRINGS.en[key] ?? key;
  return template.replace(/\{(\w+)\}/g, (_, name) => (values && name in values ? values[name] : ""));
}

const elements = {
  saved: document.getElementById("saved"),
  empty: document.getElementById("empty"),
  list: document.getElementById("list"),
  add: document.getElementById("add"),
  modal: document.getElementById("modal"),
  modalTitle: document.getElementById("modal-title"),
  close: document.getElementById("close"),
  cancel: document.getElementById("cancel"),
  form: document.getElementById("form"),
  name: document.getElementById("name"),
  url: document.getElementById("url"),
  username: document.getElementById("username"),
  password: document.getElementById("password"),
  reveal: document.getElementById("reveal"),
  submit: document.getElementById("submit"),
  status: document.getElementById("status"),
  error: document.getElementById("error"),
  done: document.getElementById("done"),
  hint: document.getElementById("hint"),
};

let accounts = [];
let autoOpened = false;
// The account the dialog is rewriting, or null when signing in to a new one.
let editing = null;

function setStatus(text, kind) {
  elements.status.textContent = text || "";
  elements.status.className = "status" + (kind ? " " + kind : "");
}

function setError(text) {
  elements.error.textContent = text || "";
}

async function request(path, options = {}) {
  const init = { method: options.method || "GET" };
  if (options.body !== undefined) {
    init.body = JSON.stringify(options.body);
    init.headers = { "content-type": "application/json" };
  }
  const response = await fetch(path, init);
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    const error = new Error("request failed");
    error.data = data;
    throw error;
  }
  return data;
}

function hostOf(url) {
  return url.replace(/^https?:\/\//i, "").replace(/\/+$/, "");
}

function openModal(account = null) {
  setError("");
  editing = account ? account.id : null;

  elements.modalTitle.textContent = editing ? t("editTitle") : t("signin");
  elements.submit.textContent = editing ? t("save") : t("connect");
  elements.password.value = "";
  elements.password.placeholder = editing ? t("keepPassword") : "";

  if (account) {
    elements.name.value = account.name;
    elements.url.value = account.url;
    elements.username.value = account.username;
  }

  if (!elements.modal.open) elements.modal.showModal();
  const target = editing
    ? elements.name
    : !elements.url.value
      ? elements.url
      : !elements.username.value
        ? elements.username
        : elements.name;
  target.focus();
}

function closeModal() {
  setError("");
  editing = null;
  if (elements.modal.open) elements.modal.close();
}

function card(account, current) {
  const item = document.createElement("li");
  item.className = "card" + (current ? " current" : "");

  const pick = document.createElement("button");
  pick.type = "button";
  pick.className = "pick";
  pick.addEventListener("click", () => useAccount(account.id));

  const avatar = document.createElement("span");
  avatar.className = "avatar";
  avatar.textContent = hostOf(account.label).charAt(0) || "?";

  const meta = document.createElement("span");
  meta.className = "meta";
  const title = document.createElement("span");
  title.className = "title";
  title.textContent = account.label;
  const detail = document.createElement("span");
  detail.className = "detail";
  detail.textContent = account.username + " \u00b7 " + hostOf(account.url);
  meta.append(title, detail);

  pick.append(avatar, meta);
  if (current) {
    const check = document.createElement("span");
    check.className = "check";
    check.textContent = "\u2713";
    pick.setAttribute("aria-current", "true");
    pick.append(check);
  }

  const edit = document.createElement("button");
  edit.type = "button";
  edit.className = "edit";
  edit.title = t("editNamed", { name: account.label });
  edit.setAttribute("aria-label", edit.title);
  edit.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20h4l10-10-4-4L4 16v4z"/></svg>';
  edit.addEventListener("click", () => openModal(account));

  const drop = document.createElement("button");
  drop.type = "button";
  drop.className = "drop";
  drop.title = t("removeConfirm", { name: account.label });
  drop.textContent = "\u00d7";
  drop.addEventListener("click", () => removeAccount(account));

  item.append(pick, edit, drop);
  return item;
}

function render(state) {
  accounts = state.accounts || [];
  elements.saved.hidden = accounts.length === 0;
  elements.empty.hidden = accounts.length > 0;
  elements.list.replaceChildren(...accounts.map((account) => card(account, account.id === state.current)));

  if (state.current) {
    const current = accounts.find((account) => account.id === state.current);
    if (current && !elements.url.value) {
      elements.url.value = current.url;
      elements.username.value = current.username;
    }
  }

  // The way out is only there once there is something to go back to.
  elements.done.hidden = !state.current;
  elements.hint.hidden = elements.done.hidden;

  // Nobody signed in yet, so the form is the only thing worth showing.
  if (accounts.length === 0 && !autoOpened) {
    autoOpened = true;
    openModal();
  }
}

async function refresh() {
  render(await request("/api/state"));
}

async function useAccount(id) {
  setStatus(t("switching"));
  try {
    const data = await request("/api/switch", { method: "POST", body: { id } });
    await refresh();
    setStatus(t("switched", { name: data.account.label }), "ok");
  } catch (error) {
    report(error);
  }
}

async function removeAccount(account) {
  if (!window.confirm(t("removeConfirm", { name: account.label }))) return;
  try {
    await request("/api/accounts/" + encodeURIComponent(account.id), { method: "DELETE" });
    await refresh();
    setStatus(t("removed", { name: account.label }), "ok");
  } catch (error) {
    report(error);
  }
}

function report(error) {
  const detail = ((error && error.data) || {}).error || {};
  const kind = detail.kind;
  let message;
  if (kind === "server") message = t("server", { message: detail.message });
  else if (kind === "duplicate") message = t("duplicate");
  else if (kind === "network" || kind === "credentials") message = t(kind);
  else message = t("unknown", { message: detail.message || error.message });

  if (elements.modal.open) setError(message);
  else setStatus(message, "error");
}

elements.add.addEventListener("click", () => openModal());
elements.close.addEventListener("click", closeModal);
elements.cancel.addEventListener("click", closeModal);
elements.modal.addEventListener("close", () => setError(""));

elements.form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const body = {
    name: elements.name.value.trim(),
    url: elements.url.value.trim(),
    username: elements.username.value.trim(),
    password: elements.password.value,
  };
  if (!body.url || !body.username || (!editing && !body.password)) {
    setError(t("missing"));
    (body.url ? (body.username ? elements.password : elements.username) : elements.url).focus();
    return;
  }

  const id = editing;
  const label = id ? t("save") : t("connect");
  elements.submit.disabled = true;
  setError("");
  elements.submit.textContent = t("connecting");
  try {
    const path = id ? "/api/accounts/" + encodeURIComponent(id) : "/api/login";
    const data = await request(path, { method: id ? "PUT" : "POST", body });
    elements.password.value = "";
    if (!id) elements.name.value = "";
    await refresh();
    closeModal();
    setStatus(id ? t("edited", { name: data.account.label }) : t("signedIn", { name: data.account.label }), "ok");
  } catch (error) {
    report(error);
  } finally {
    elements.submit.disabled = false;
    elements.submit.textContent = label;
  }
});

elements.reveal.addEventListener("click", () => {
  const hidden = elements.password.type === "password";
  elements.password.type = hidden ? "text" : "password";
  elements.reveal.textContent = t(hidden ? "hide" : "show");
});

elements.done.addEventListener("click", () => {
  elements.done.disabled = true;
  setStatus(t("signingOut"));
  // The card there hands the account to the plugin and then tries to close.
  location.href = "/done";
});

document.documentElement.lang = language === "zh" ? "zh-CN" : "en";
document.title = t("title");
for (const node of document.querySelectorAll("[data-i18n]")) {
  node.textContent = t(node.dataset.i18n);
}
elements.reveal.textContent = t("show");

refresh().catch(report);
