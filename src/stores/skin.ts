// 着せ替え（v3.2.0）の写し。正は Rust（skins::runtime）。ここは当てるだけ。
// - 色と文字：今のテーマに合わせて、許した変数だけを <html> に当てる（既定のスキンでは何も当てない＝今の見た目そのまま）
// - v3.10.0：手で選んだ色（設定の skin.colors）をスキンの色の上に重ねる。手の色が無ければ 3.9.1 と同じ（何も足さない）。
//   セーフモードの間は手の色も当てない（既定の見た目）
// - 「動きを減らす」（prefers-reduced-motion）を見張り、変わったらすぐ反映する（動く画像の代わりに最初の1コマを読む）
import { api } from "../ipc/commands";
import { fileUrl, on } from "../ipc/events";
import type { AvatarList, PickedImage, ResolvedSkin, SkinCharacter, SkinInspection, SkinState } from "../ipc/types";
import { skinNoticeText } from "../messages/ja";
import { applyCssVars, layeredCssVars, manualColorVars } from "../skins/tokens";
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

/** v3.10.0：色の選び窓を動かしている間だけの色（保存は選び窓を閉じたとき・つまみを離したとき）。手の色の上に重ねて当てる */
export const colorPreview = createStore<Record<string, string>>({});

/** 手で選んだ色（セーフモードの間は当てない）。動かしている途中の色を上に重ねる */
function manualColorsNow(): Record<string, string> {
  if (skinState.get()?.safeMode) return {};
  return { ...(settings.get().skin?.colors ?? {}), ...colorPreview.get() };
}

/** 知らせた手の色の不合格（同じものを何度も知らせない） */
let lastManualRejected = "";

/**
 * 今のテーマに、スキンの色と文字・手で選んだ色を当てる。スキンの値が確かめに通らなければ、スキンを既定に戻して知らせる
 * （手の色は残す。「既定の見た目に戻す」とは別の道）。手の色が確かめに通らなければ、その色だけ当てずに知らせる
 */
function applyTokens() {
  const root = document.documentElement;
  const theme = settings.get().theme || "dark";
  const { vars, rejected, manualRejected } = layeredCssVars(currentSkin()?.appearance ?? null, theme, manualColorsNow());
  const bad = manualRejected.join(", ");
  if (bad && bad !== lastManualRejected) notify({ level: "error", error: { code: "skin_color_invalid", detail: bad } });
  lastManualRejected = bad;
  if (rejected.length > 0) {
    applyCssVars(root.style, manualColorVars(manualColorsNow()).vars);
    notify({ level: "error", error: { code: "skin_apply_failed", detail: rejected.join(", ") } });
    void fallbackToDefaultSkin();
    return;
  }
  applyCssVars(root.style, vars);
}

/** 当てているスキンの値が読めなかったとき：スキンだけを既定に戻す（手で選んだ色と立ち絵は残す） */
async function fallbackToDefaultSkin() {
  try {
    receive(await api.skinsApply("builtin:default"));
  } catch (e) {
    fail(e);
  }
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
  // テーマを変えたら、そのテーマの色に当て直す。v3.10.0：手で選んだ色が変わったときも（Rust が保存して知らせた設定）
  currentTheme.set(settings.get().theme || "dark");
  let colorsKey = JSON.stringify(settings.get().skin?.colors ?? {});
  settings.subscribe(() => {
    const t = settings.get().theme || "dark";
    const k = JSON.stringify(settings.get().skin?.colors ?? {});
    if (t !== currentTheme.get() || k !== colorsKey) {
      currentTheme.set(t);
      colorsKey = k;
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

/** 「既定の見た目に戻す」：スキンを既定にし、手で選んだ色も外す（v3.10.0）。立ち絵は残す */
export async function resetSkin() {
  try {
    receive(await api.skinsReset());
  } catch (e) {
    fail(e);
  }
}

// ---------------- 手で選ぶ色（v3.10.0） ----------------

/** 選び窓・つまみを動かしている間：保存せずに当てるだけ（値は当てる前に tokens.ts の決まりで確かめる） */
export function previewColor(name: string, value: string) {
  colorPreview.set((p) => ({ ...p, [name]: value }));
  applyTokens();
}

/** 動かしていた色を捨てる（保存しなかったとき）。当て直す */
export function dropColorPreview(name: string) {
  if (!(name in colorPreview.get())) return;
  colorPreview.set((p) => {
    const { [name]: _drop, ...rest } = p;
    return rest;
  });
  applyTokens();
}

/** 手で選んだ色を1つ保存する（null なら外す）。Rust が確かめて形をそろえる。断られたら理由を出し、前の色に戻す */
export async function commitColor(name: string, value: string | null) {
  try {
    settings.set(await api.skinsColorSet(name, value));
  } catch (e) {
    fail(e);
  } finally {
    dropColorPreview(name);
  }
}

/** 手で選んだ色を全部外す（スキンと立ち絵は変えない） */
export async function clearColors() {
  try {
    settings.set(await api.skinsColorsClear());
  } catch (e) {
    fail(e);
  }
  colorPreview.set({});
  applyTokens();
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

// ---------------- 立ち絵（v3.9.0） ----------------

/** いま画面に出す立ち絵（Rust が使う立ち絵を解いたもの。出さないときは null） */
export const useCharacter = (): SkinCharacter | null => useStore(skinState)?.character ?? null;

/** 設定の「立ち絵」の欄の一覧 */
export const avatarList = createStore<AvatarList | null>(null);

export async function loadAvatars() {
  try {
    avatarList.set(await api.avatarsList());
  } catch (e) {
    fail(e);
  }
}

/** 使う立ち絵を選ぶ（"skin"・"none"・"avatar:<id>"・"skin:<id>"） */
export async function chooseAvatar(choice: string) {
  try {
    receive(await api.avatarsUse(choice));
  } catch (e) {
    fail(e);
    void loadAvatars();
  }
}

/** 立ち絵を出さない（v3.9.0 直し1回目。avatars_use の "none" と同じ道。作った立ち絵は消さない） */
export async function hideAvatar() {
  try {
    receive(await api.avatarsHide());
  } catch (e) {
    fail(e);
  }
}

export async function deleteAvatar(id: string) {
  try {
    receive(await api.avatarsDelete(id));
  } catch (e) {
    fail(e);
  }
  void loadAvatars();
}

/** 作った立ち絵を変える（v3.9.0 直し2回目。差し替え・停止中を外す・名前）。使っている立ち絵なら、Rust が画面の絵も当て直す。断られたら理由を出して false */
export async function updateAvatar(id: string, name: string, playing: string | null, stopped: string | null, removeStopped: boolean): Promise<boolean> {
  try {
    await api.avatarsUpdate(id, name, playing, stopped, removeStopped);
    void loadAvatars();
    return true;
  } catch (e) {
    fail(e);
    return false;
  }
}

/** 画像を選ばせて確かめる（選ばなければ null。断られたら理由を出して null） */
export async function pickAvatarImage(slot: "playing" | "stopped"): Promise<PickedImage | null> {
  try {
    return await api.avatarsPickImage(slot);
  } catch (e) {
    fail(e);
    return null;
  }
}

export async function cancelAvatarPick() {
  await api.avatarsCancelPick().catch(() => {});
}

/** 立ち絵を作る（使う立ち絵は変えない）。断られたら理由を出して false */
export async function createAvatar(name: string, playing: string, stopped: string | null): Promise<boolean> {
  try {
    const a = await api.avatarsCreate(name, playing, stopped);
    notify({ level: "info", text: `立ち絵「${a.name}」を追加しました。出すときは一覧のカードか「使う」を押してください。` });
    void loadAvatars();
    return true;
  } catch (e) {
    fail(e);
    return false;
  }
}

/** アイコンの画像の URL（スキンに無ければ null。呼び出し側は今の SVG を出す） */
export function useSkinIcon(name: string, state: string): string | null {
  const skin = useSkin();
  const theme = useStore(currentTheme);
  const reduce = useStore(reducedMotion);
  if (!skin) return null;
  return fileUrl(pickIconPath(skin.icons, name, state, theme, reduce));
}
