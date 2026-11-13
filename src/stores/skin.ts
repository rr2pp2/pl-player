// 着せ替え（v3.2.0）の写し。正は Rust（skins::runtime）。ここは当てるだけ。
// - 色と文字：今のテーマに合わせて、許した変数だけを <html> に当てる（既定のスキンでは何も当てない＝今の見た目そのまま）
// - 「動きを減らす」（prefers-reduced-motion）を見張り、変わったらすぐ反映する（動く画像の代わりに最初の1コマを読む）
import { api } from "../ipc/commands";
import { fileUrl, on } from "../ipc/events";
import type { ResolvedSkin, SkinInspection, SkinState } from "../ipc/types";
import { skinNoticeText } from "../messages/ja";
import { applyCssVars, skinCssVars } from "../skins/tokens";
import { pickIconPath } from "../skins/icons";
import { settings } from "./settings";
import { playback } from "./playback";
import { createStore, useStore } from "./store";
import { notify } from "./notices";

export const skinState = createStore<SkinState | null>(null);

const REDUCE = "(prefers-reduced-motion: reduce)";
export const reducedMotion = createStore<boolean>(typeof window !== "undefined" && window.matchMedia ? window.matchMedia(REDUCE).matches : false);

/** 今のテーマ（アイコンが設定全体の変化で描き直されないよう、テーマだけを持つ） */
export const currentTheme = createStore<string>("dark");

/** 再生中か（立ち絵の絵を選ぶ。再生の写し全体ではなく、この真偽だけを見て描き直す） */
export const playingNow = createStore<boolean>(false);

/** 取り込みの確認画面に出しているもの */
export const skinImport = createStore<SkinInspection | null>(null);

/** zip を選んで確かめている途中（「zip を取り込む…」を押せなくする。Rust 側も1本ずつにしている） */
export const skinPicking = createStore(false);

/** 一覧を読み直す合図（取り込み・削除のあと） */
export const skinListVersion = createStore(0);
export const bumpSkinList = () => skinListVersion.set((v) => v + 1);

const currentSkin = (): ResolvedSkin | null => skinState.get()?.skin ?? null;

/** 今のテーマに、スキンの色と文字を当てる。確かめに通らない値があれば、既定の見た目に戻して知らせる */
function applyTokens() {
  const root = document.documentElement;
  const theme = settings.get().theme || "dark";
  const { vars, rejected } = skinCssVars(currentSkin()?.appearance ?? null, theme);
  if (rejected.length > 0) {
    applyCssVars(root.style, {});
    notify({ level: "error", error: { code: "skin_apply_failed", detail: rejected.join(", ") } });
    void resetSkin();
    return;
  }
  applyCssVars(root.style, vars);
}

function receive(s: SkinState) {
  skinState.set(s);
  applyTokens();
  if (s.notice) {
    const m = skinNoticeText(s.notice);
    notify({ level: m.level, text: m.text, detail: m.detail });
  }
}

export async function initSkins() {
  receive(await api.skinsState());
  await on("skin", receive);
  // テーマを変えたら、そのテーマの色に当て直す
  currentTheme.set(settings.get().theme || "dark");
  settings.subscribe(() => {
    const t = settings.get().theme || "dark";
    if (t !== currentTheme.get()) {
      currentTheme.set(t);
      applyTokens();
    }
  });
  playingNow.set(playback.get().status === "playing");
  playback.subscribe(() => playingNow.set(playback.get().status === "playing"));
  if (window.matchMedia) {
    window.matchMedia(REDUCE).addEventListener("change", (e) => reducedMotion.set(e.matches));
  }
}

const fail = (e: unknown) => notify({ level: "error", error: e });

export async function applySkin(id: string) {
  try {
    receive(await api.skinsApply(id));
    return true;
  } catch (e) {
    fail(e);
    return false;
  }
}

export async function resetSkin() {
  try {
    receive(await api.skinsReset());
  } catch (e) {
    fail(e);
  }
}

export async function exitSafeMode() {
  try {
    receive(await api.skinsExitSafeMode());
  } catch (e) {
    fail(e);
    bumpSkinList();
  }
}

export async function deleteSkin(id: string) {
  try {
    receive(await api.skinsDelete(id));
  } catch (e) {
    fail(e);
  }
  bumpSkinList();
}

export async function exportSkin(id: string) {
  try {
    const r = await api.skinsExport(id);
    if (r) notify({ level: "info", text: `スキンを書き出しました（${r.files} ファイル）` });
  } catch (e) {
    fail(e);
  }
}

export async function pickSkinZip() {
  if (skinPicking.get()) return;
  skinPicking.set(true);
  try {
    const r = await api.skinsPickAndInspect();
    if (r) skinImport.set(r);
  } catch (e) {
    fail(e);
  } finally {
    skinPicking.set(false);
  }
}

export async function cancelSkinImport() {
  const r = skinImport.get();
  skinImport.set(null);
  if (r) await api.skinsImportCancel(r.token).catch(() => {});
}

export async function commitSkinImport() {
  const r = skinImport.get();
  if (!r) return;
  try {
    const s = await api.skinsImportCommit(r.token);
    skinImport.set(null);
    notify({ level: "info", text: `スキン「${s.name}」を取り込みました。使うときは一覧の「適用」を押してください。` });
  } catch (e) {
    fail(e);
  }
  bumpSkinList();
}

export const useSkin = (): ResolvedSkin | null => useStore(skinState)?.skin ?? null;

/** アイコンの画像の URL（スキンに無ければ null。呼び出し側は今の SVG を出す） */
export function useSkinIcon(name: string, state: string): string | null {
  const skin = useSkin();
  const theme = useStore(currentTheme);
  const reduce = useStore(reducedMotion);
  if (!skin) return null;
  return fileUrl(pickIconPath(skin.icons, name, state, theme, reduce));
}
