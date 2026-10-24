
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { init, t, lang, stripErr } from "./i18n.js";

const SDK_URL = "https://o.alicdn.com/captcha-frontend/aliyunCaptcha/AliyunCaptcha.js";
const TRACELESS_TIMEOUT = 8000;

const $text = document.getElementById("cap-text");
const $detail = document.getElementById("cap-detail");
const $dot = document.getElementById("cap-dot");
const $btn = document.getElementById("cap-btn");

function log(msg) {
  try { invoke("captcha_log", { msg: String(msg) }).catch(() => {}); } catch { }
}
const j = (x) => { try { return typeof x === "string" ? x : JSON.stringify(x); } catch { return String(x); } };
window.addEventListener("error", (e) => log(`window.error: ${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => log(`unhandledrejection: ${j(e.reason?.message || e.reason)}`));
window.addEventListener("beforeunload", () => log(`beforeunload (submitted=${submitted})`));

function status(text, tone = "run") {
  log(`status[${tone}]: ${text}`);
  $text.textContent = text;
  $dot.className = "cap-dot" + (tone === "ok" ? " ok" : tone === "err" ? " err" : "");
}

function detail(text) {
  if (text) log(`detail: ${text}`);
  $detail.textContent = text || "";
}

document.addEventListener("securitypolicyviolation", (e) => {
  log(`CSP violation: directive=${e.violatedDirective} blocked=${String(e.blockedURI).slice(0, 200)} source=${e.sourceFile}:${e.lineNumber}`);
  detail(t("c.cspBlocked", { directive: e.violatedDirective, uri: String(e.blockedURI).slice(0, 70) }));
});

const notifyStuck = () => { emit("captcha://interactive").catch(() => {}); };

function loadSdk() {
  return new Promise((resolve, reject) => {
    if (typeof window.initAliyunCaptcha === "function") return resolve();
    const s = document.createElement("script");
    s.src = SDK_URL;
    s.onload = () => resolve();
    s.onerror = () => reject(new Error(t("c.sdkFail")));
    document.head.appendChild(s);
  });
}

let submitted = false;
let region = null;
let tracelessTimer = 0;

async function run() {
  try {
    const st = await invoke("get_state");
    if (st?.language) init(st.language);
  } catch { }
  document.title = t("c.title");
  $btn.textContent = t("c.btn");
  document.querySelector(".cap-foot").textContent = t("c.foot");
  status(t("c.preparing"));

  let cfg;
  try {
    cfg = await invoke("claim_captcha_config");
  } catch (e) {
    notifyStuck();
    status(t("c.cfgFail"), "err");
    detail(stripErr(e));
    return;
  }
  if (!cfg.enabled || !cfg.scene_id) {
    notifyStuck();
    status(t("c.cfgUnavailable"), "err");
    detail(t("c.cfgUnavailableDetail"));
    return;
  }
  region = cfg.region || null;
  try {
    await loadSdk();
  } catch (e) {
    notifyStuck();
    status(e.message || t("c.sdkFail"), "err");
    return;
  }

  window.AliyunCaptchaConfig = { region: cfg.region, prefix: cfg.prefix };

  status(t("c.traceless"));

  const submit = (param) => {
    log(`success callback: param_len=${param ? String(param).length : 0} submitted=${submitted}`);
    if (submitted || !param || !param.trim()) return;
    submitted = true;
    clearTimeout(tracelessTimer);
    status(t("c.passed"));
    invoke("claim_captcha_submit", { param, region }).then((r) => log(`claim_captcha_submit returned: ${j(r).slice(0, 400)}`)).catch((e) => {
      log(`claim_captcha_submit error: ${j(e)}`);
      status(t("c.claimReqFail"), "err");
      detail(stripErr(e));
    });
  };

  const interactive = (why) => {
    log(`interactive fallback: ${why === undefined ? "traceless timeout" : j(why).slice(0, 400)}`);
    notifyStuck();
    clearTimeout(tracelessTimer);
    status(t("c.interactive"));
    $btn.hidden = false;
    $btn.focus();
    if (why) detail(typeof why === "string" ? why.slice(0, 120) : JSON.stringify(why).slice(0, 120));
  };

  try {
    window.initAliyunCaptcha({
      SceneId: cfg.scene_id,
      mode: "popup",
      language: lang() === "en" ? "en" : "zh-CN",
      showErrorTip: false,
      element: "#cap-holder",
      button: "#cap-btn",
      getInstance: (instance) => {
        log(`getInstance: traceless=${typeof instance.startTracelessVerification}`);
        if (typeof instance.startTracelessVerification === "function") {
          instance.startTracelessVerification();
          tracelessTimer = setTimeout(interactive, TRACELESS_TIMEOUT);
        } else {
          interactive();
        }
      },
      success: (param) => submit(typeof param === "string" ? param : param?.captchaVerifyParam),
      fail: (p) => { log(`fail callback: ${j(p).slice(0, 400)}`); interactive(p); },
      onError: (p) => { log(`onError callback: ${j(p).slice(0, 400)}`); interactive(p); },
      onClose: () => log("captcha popup onClose"),
    });
  } catch (e) {
    notifyStuck();
    status(t("c.initFail"), "err");
    detail(String(e));
  }
}

$btn.addEventListener("click", () => {
  if (!$btn.hidden) status(t("c.inPopup"));
});

log(`captcha.js loaded href=${location.href}`);
run();
