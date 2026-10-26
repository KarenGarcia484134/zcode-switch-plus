// 浅色 / 深色 / 跟随系统 主题：持久化到 localStorage（main/settings/captcha 同源共享），
// 跨窗口通过 storage 事件同步，并同步 Tauri 原生窗口主题（标题栏）。
import { getCurrentWindow } from "@tauri-apps/api/window";

const KEY = "zcs.theme";
const mq = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;

export function getThemePref() {
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "dark" ? v : "system";
}

function resolved(pref) {
  if (pref === "system") return mq && mq.matches ? "dark" : "light";
  return pref;
}

let applying = false;
export function applyTheme() {
  const pref = getThemePref();
  const eff = resolved(pref);
  document.documentElement.dataset.theme = eff;
  document.documentElement.style.colorScheme = eff;
  if (applying) return;
  applying = true;
  try {
    getCurrentWindow().setTheme(pref === "system" ? null : pref).catch(() => {}).finally(() => { applying = false; });
  } catch { applying = false; }
}

export function setThemePref(pref) {
  if (pref === "system") localStorage.removeItem(KEY);
  else localStorage.setItem(KEY, pref);
  applyTheme();
}

export function cycleTheme() {
  const order = ["system", "light", "dark"];
  const next = order[(order.indexOf(getThemePref()) + 1) % order.length];
  setThemePref(next);
  return next;
}

export function onThemeChange(cb) {
  window.addEventListener("storage", (e) => { if (e.key === KEY) { applyTheme(); cb && cb(); } });
  mq && mq.addEventListener && mq.addEventListener("change", () => { applyTheme(); cb && cb(); });
}

applyTheme();
