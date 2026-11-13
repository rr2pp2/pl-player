// 設定：表示（テーマ・背景）・着せ替え（v3.2.0）・再生デバイス・ライブラリ・歌詞をネットから取る（v3.1.0）・ショートカット・グローバルホットキー・ウィンドウ・バックアップ・情報。
import { useEffect, useState } from "react";
import { api } from "../ipc/commands";
import { on } from "../ipc/events";
import type { AppInfo, BackupReport, DeviceInfo, FolderEntry } from "../ipc/types";
import { playback } from "../stores/playback";
import { libraryVersion } from "../stores/library";
import { notify } from "../stores/notices";
import { lyricsOnlineOn, settings, updateSettings } from "../stores/settings";
import { importLyricsInbox, lastInboxReport, openLyricsInbox } from "../stores/lyrics";
import { cancelLyricsFetch, lyricsFetch, lyricsFetchOne, lyricsFetchRunning, startLyricsFetch } from "../stores/lyricsFetch";
import { inboxReportText, lyricsFetchAbortMessage, lyricsFetchProgressText } from "../messages/ja";
import type { LyricsFetchMode } from "../ipc/types";
import { useStore } from "../stores/store";
import { Artwork } from "../components/Artwork";
import { dropState, IMAGE_RE, registerDropZone } from "../components/FileDrop";
import { HotkeyEditor, ShortcutEditor } from "../components/ShortcutEditor";
import { confirmDialog } from "../components/Modal";
import { IconFolder, IconFolderPlus, IconImage, IconRefresh, IconTrash, IconX } from "../components/Icons";
import { SkinsSection } from "./SettingsSkins";

const BG_ZONE = "settings-bg";
const THEMES = [
  { id: "dark", label: "Dark" },
  { id: "light", label: "Light" },
];
const COLS = [
  { id: "album", label: "アルバム" },
  { id: "genre", label: "ジャンル" },
  { id: "duration", label: "時間" },
  { id: "favorite", label: "お気に入り" },
];

function Slider(props: { value: number; min: number; max: number; step: number; onChange: (v: number) => void; fmt: (v: number) => string; label: string }) {
  const [local, setLocal] = useState<number | null>(null);
  const v = local ?? props.value;
  const pct = ((v - props.min) / (props.max - props.min)) * 100;
  return (
    <span className="row" style={{ flexWrap: "nowrap" }}>
      <input
        type="range"
        className="range"
        aria-label={props.label}
        min={props.min}
        max={props.max}
        step={props.step}
        value={v}
        style={{ ["--pct" as any]: `${pct}%`, maxWidth: 320 }}
        onChange={(e) => {
          const nv = Number(e.target.value);
          setLocal(nv);
          // 見た目はすぐ変える（保存は手を離したとき）
          settings.set((s) => ({ ...s, background: { ...s.background, ...previewPatch(props.label, nv) } }));
        }}
        onPointerUp={() => {
          if (local !== null) props.onChange(local);
          setLocal(null);
        }}
        onKeyUp={() => {
          if (local !== null) props.onChange(local);
          setLocal(null);
        }}
      />
      <span style={{ width: 48, textAlign: "right", fontVariantNumeric: "tabular-nums" }}>{props.fmt(v)}</span>
    </span>
  );
}

function previewPatch(label: string, v: number) {
  if (label === "背景の暗さ") return { dim: v };
  if (label === "ぼかし") return { blur: v };
  return { uiOpacity: v };
}

function report(r: BackupReport | null, kind: "export" | "import") {
  if (!r) return;
  if (kind === "export") {
    notify({ level: "info", text: `設定を書き出しました（プレイリスト${r.playlists}件・お気に入り${r.favorites}曲・フォルダ${r.folders}件）` });
  } else {
    notify({ level: "info", text: `設定を読み込みました（プレイリスト${r.playlists}件・フォルダ${r.folders}件）` });
    if (r.foldersRejected > 0)
      notify({ level: "error", text: `ネットワーク上の場所なので、ライブラリのフォルダ ${r.foldersRejected} 件を入れませんでした。` });
    if (r.backgroundRejected) notify({ level: "error", text: "背景画像は画像として読めなかったので、読み込みませんでした。" });
    const miss = r.playlistTracksMissing + r.favoritesMissing;
    if (miss > 0)
      notify({
        level: "error",
        text: `この PC のライブラリに見つからない曲が ${miss} 件ありました（プレイリスト ${r.playlistTracksMissing}・お気に入り ${r.favoritesMissing}）。曲の場所が違う可能性があります。`,
      });
  }
}

export function SettingsView() {
  const s = useStore(settings);
  const pb = useStore(playback);
  const drop = useStore(dropState);
  const lver = useStore(libraryVersion);
  const inbox = useStore(lastInboxReport);
  const [importingLyrics, setImportingLyrics] = useState(false);
  const fetchProg = useStore(lyricsFetch);
  const fetchOne = useStore(lyricsFetchOne);
  const [fetchMode, setFetchMode] = useState<LyricsFetchMode>("missing");
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [folders, setFolders] = useState<FolderEntry[]>([]);
  const [info, setInfo] = useState<AppInfo | null>(null);

  const loadDevices = () => api.devices().then(setDevices).catch((e) => notify({ level: "error", error: e }));
  useEffect(() => {
    void loadDevices();
    let un: (() => void) | undefined;
    let t: number | undefined;
    void on("devices", () => {
      window.clearTimeout(t);
      t = window.setTimeout(() => void loadDevices(), 300);
    }).then((f) => (un = f));
    return () => {
      un?.();
      window.clearTimeout(t);
    };
  }, []);
  useEffect(() => {
    api.libraryFolders().then(setFolders).catch(() => {});
    api.appInfo().then(setInfo).catch(() => {});
  }, [lver]);

  const setBg = async (path: string) => {
    try {
      await api.backgroundSet(path);
    } catch (e) {
      notify({ level: "error", error: e });
    }
  };
  useEffect(
    () =>
      registerDropZone(BG_ZONE, (paths) => {
        const img = paths.find((p) => IMAGE_RE.test(p));
        if (img) void setBg(img);
        else notify({ level: "error", text: "背景には PNG・JPG・JPEG・WebP の画像を使えます。" });
      }),
    [],
  );

  const bg = s.background;
  return (
    <div className="content">
      <div className="view-head">
        <h1 className="view-title">設定</h1>
      </div>
      <div className="settings">
        <section>
          <h3>表示</h3>
          <div className="field">
            <span className="flabel">テーマ</span>
            <span className="seg" role="radiogroup">
              {THEMES.map((t) => (
                <button key={t.id} className={s.theme === t.id ? "on" : ""} onClick={() => updateSettings((x) => ({ ...x, theme: t.id }))}>
                  {t.label}
                </button>
              ))}
            </span>
          </div>
          <div className="field">
            <span className="flabel">背景画像</span>
            <span className="row">
              <Artwork path={bg.path} className="album-head-art" dropTarget={BG_ZONE} dropActive={drop.active && drop.zone === BG_ZONE} />
              <span className="row" style={{ flexDirection: "column", alignItems: "flex-start" }}>
                <button
                  className="btn"
                  onClick={async () => {
                    const p = await api.pickImage("背景画像を選ぶ");
                    if (p) void setBg(p);
                  }}
                >
                  <IconImage /> 画像を選ぶ…
                </button>
                <button className="btn" disabled={!bg.path} onClick={() => api.backgroundClear().catch((e) => notify({ level: "error", error: e }))}>
                  <IconX /> 背景を外す
                </button>
              </span>
            </span>
            <span className="hint">PNG・JPG・JPEG・WebP。枠へドラッグ＆ドロップでも設定できます。画像はアプリのデータフォルダへ複製します。</span>
          </div>
          <div className="field">
            <span className="flabel">表示のしかた</span>
            <span className="seg">
              <button className={bg.fit !== "contain" ? "on" : ""} onClick={() => updateSettings((x) => ({ ...x, background: { ...x.background, fit: "cover" } }))}>
                Cover（画面いっぱい）
              </button>
              <button className={bg.fit === "contain" ? "on" : ""} onClick={() => updateSettings((x) => ({ ...x, background: { ...x.background, fit: "contain" } }))}>
                全体を表示
              </button>
            </span>
          </div>
          <div className="field">
            <span className="flabel">背景の暗さ</span>
            <Slider label="背景の暗さ" value={bg.dim} min={0} max={0.95} step={0.01} fmt={(v) => `${Math.round(v * 100)}%`} onChange={(v) => updateSettings((x) => ({ ...x, background: { ...x.background, dim: v } }))} />
          </div>
          <div className="field">
            <span className="flabel">ぼかし</span>
            <Slider label="ぼかし" value={bg.blur} min={0} max={40} step={1} fmt={(v) => `${v}px`} onChange={(v) => updateSettings((x) => ({ ...x, background: { ...x.background, blur: v } }))} />
          </div>
          <div className="field">
            <span className="flabel">UI の不透明度</span>
            <Slider label="UI の不透明度" value={bg.uiOpacity} min={0.3} max={1} step={0.01} fmt={(v) => `${Math.round(v * 100)}%`} onChange={(v) => updateSettings((x) => ({ ...x, background: { ...x.background, uiOpacity: v } }))} />
            <span className="hint">文字が読みにくいときは、暗さか不透明度を上げてください。</span>
          </div>
          <div className="field">
            <span className="flabel">曲一覧の列</span>
            <span className="row">
              {COLS.map((c) => (
                <label key={c.id} className="row" style={{ gap: 4 }}>
                  <input
                    type="checkbox"
                    checked={s.ui.columns[c.id] !== false}
                    onChange={(e) => updateSettings((x) => ({ ...x, ui: { ...x.ui, columns: { ...x.ui.columns, [c.id]: e.target.checked } } }))}
                  />
                  {c.label}
                </label>
              ))}
            </span>
            <span className="hint">ウィンドウが狭いときは、ジャンル → アルバムの順に自動で隠れます。</span>
          </div>
        </section>

        <SkinsSection />

        <section>
          <h3>再生デバイス</h3>
          <div className="field">
            <span className="flabel">出力先</span>
            <div className="list-box" role="radiogroup" aria-label="出力先">
              {[{ id: "", name: "既定のデバイス（Windows の設定に従う）", isDefault: false }, ...devices].map((d) => {
                const on = (pb.deviceId ?? "") === d.id;
                return (
                  <button
                    key={d.id || "default"}
                    role="radio"
                    aria-checked={on}
                    className={"li device-item" + (on ? " on" : "")}
                    onClick={() => api.setDevice(d.id || null).catch((err) => notify({ level: "error", error: err }))}
                  >
                    <span className="radio-dot" />
                    <span className="grow">
                      {d.name}
                      {d.isDefault ? "（今の既定）" : ""}
                    </span>
                  </button>
                );
              })}
              {devices.length === 0 ? <div className="li path-text">出力デバイスが見つかりません。「一覧を更新」を押してください。</div> : null}
            </div>
            <span className="hint">
              いま使っているデバイス：{pb.deviceName || "（まだ開いていません）"}。再生中に変えても、曲と位置はそのままです。切断されたら既定のデバイスで続けます。
            </span>
          </div>
          <div className="field">
            <span className="flabel" />
            <span>
              <button className="btn" onClick={() => void loadDevices()}>
                <IconRefresh /> 一覧を更新
              </button>
            </span>
          </div>
        </section>

        <section>
          <h3>ライブラリ</h3>
          <div className="field">
            <span className="flabel">フォルダ</span>
            <div>
              {folders.length ? (
                <div className="list-box">
                  {folders.map((f) => (
                    <div className="li" key={f.path}>
                      <span className="grow" title={f.path}>
                        {f.path}
                      </span>
                      <button
                        className="icon-btn small"
                        title="このフォルダの記録を外す（取り込んだ曲は残ります）"
                        onClick={async () => {
                          await api.libraryRemoveFolder(f.path);
                          setFolders(await api.libraryFolders());
                        }}
                      >
                        <IconTrash />
                      </button>
                    </div>
                  ))}
                </div>
              ) : (
                <span className="path-text">まだフォルダを追加していません。</span>
              )}
            </div>
          </div>
          <div className="field">
            <span className="flabel" />
            <span className="row">
              <button className="btn" onClick={() => void api.libraryPickFolder()}>
                <IconFolderPlus /> フォルダを追加
              </button>
              <button className="btn" onClick={() => api.libraryRescan().catch((e) => notify({ level: "error", error: e }))}>
                <IconRefresh /> 再走査
              </button>
            </span>
            <span className="hint">音声ファイルはコピーしません。元の場所を参照します。再走査では、新しい曲を取り込み、無くなった曲に印を付けます。</span>
          </div>
          <div className="field">
            <span className="flabel">外部歌詞</span>
            <span className="row">
              <button
                className="btn"
                disabled={importingLyrics}
                onClick={async () => {
                  setImportingLyrics(true);
                  await importLyricsInbox();
                  setImportingLyrics(false);
                }}
              >
                <IconRefresh /> 外部歌詞を取り込む
              </button>
              <button className="btn" onClick={() => void openLyricsInbox()}>
                <IconFolder /> 受け渡しフォルダを開く
              </button>
            </span>
            {inbox ? (
              <span className="hint" role="status">
                {inboxReportText(inbox).text}
                {inboxReportText(inbox).problems ? `（${inboxReportText(inbox).problems}）` : ""}
              </span>
            ) : null}
            <span className="hint">
              受け渡しフォルダ（データの場所の lyrics\inbox）に置いた「曲ID.lrc」「曲ID.txt」を取り込みます。取り込みはこのボタンを押したときだけです。手動で直した歌詞は上書きしません。
            </span>
          </div>
          <div className="field">
            <span className="flabel">起動時</span>
            <label className="row" style={{ gap: 6 }}>
              <input type="checkbox" checked={s.rescanOnStart} onChange={(e) => updateSettings((x) => ({ ...x, rescanOnStart: e.target.checked }))} />
              起動したときにフォルダを再走査する
            </label>
          </div>
        </section>

        <section>
          <h3>歌詞をネットから取る</h3>
          <div className="field">
            <span className="flabel">ネットから取る</span>
            <label className="row" style={{ gap: 6 }}>
              <input
                type="checkbox"
                checked={lyricsOnlineOn(s)}
                onChange={(e) => updateSettings((x) => ({ ...x, lyricsOnline: e.target.checked }))}
              />
              歌詞をネットから取る（LRCLIB）
            </label>
            <span className="hint">
              既定はオフです。オフの間、アプリは外へ一切通信しません。オンにしても、通信するのは下の「歌詞を取ってくる」か、歌詞画面の「ネットから探す」を押したときだけです。
            </span>
            <span className="hint lyrics-online-terms">
              <span>送り先：lrclib.net（無料の歌詞データベース LRCLIB）</span>
              <span>送るもの：曲のタグの曲名・アーティスト名・アルバム名と、曲の長さ（秒）</span>
              <span>送らないもの：ファイルの場所・ファイル名・そのほかの情報（再生の記録・ライブラリのほかの曲など）。タグに曲名が無く、ファイル名を曲名として表示している曲は送りません</span>
            </span>
          </div>
          <div className="field">
            <span className="flabel">取ってくる曲</span>
            <span className="seg" role="radiogroup" aria-label="取ってくる曲">
              <button role="radio" aria-checked={fetchMode === "missing"} className={fetchMode === "missing" ? "on" : ""} onClick={() => setFetchMode("missing")}>
                歌詞の無い曲だけ
              </button>
              <button role="radio" aria-checked={fetchMode === "refetch"} className={fetchMode === "refetch" ? "on" : ""} onClick={() => setFetchMode("refetch")}>
                外部の歌詞を取り直す
              </button>
            </span>
            <span className="hint">
              {fetchMode === "missing"
                ? "手動・外部・曲のタグのどれにも歌詞が無い曲だけを探します。前に見つからなかった曲は聞き直しません（曲名などを直した曲は聞き直します）。"
                : "外部の歌詞がある曲と、前に見つからなかった曲も聞き直します。取り直せなかった曲は、今の歌詞を残します。"}
              手で直した歌詞（手動）は上書きしません。曲のタグに歌詞がある曲と、タグに曲名かアーティスト名が無い曲は送りません。
            </span>
          </div>
          <div className="field">
            <span className="flabel" />
            <span className="row">
              <button
                className="btn primary"
                disabled={!lyricsOnlineOn(s) || lyricsFetchRunning(fetchProg) || fetchOne !== null}
                onClick={() => void startLyricsFetch(fetchMode)}
              >
                <IconRefresh /> 歌詞を取ってくる
              </button>
              {lyricsFetchRunning(fetchProg) ? (
                <button className="btn" onClick={() => void cancelLyricsFetch()}>
                  <IconX /> やめる
                </button>
              ) : null}
            </span>
            {!lyricsOnlineOn(s) ? <span className="hint">上の「歌詞をネットから取る」をオンにすると押せます。</span> : null}
            {fetchProg ? (
              <span className="hint" role="status">
                {fetchProg.running ? "取っています：" : fetchProg.aborted ? "止めました：" : "終わりました："}
                {lyricsFetchProgressText(fetchProg)}
                {!fetchProg.running && fetchProg.aborted ? `（${lyricsFetchAbortMessage(fetchProg.aborted, fetchProg.lastError)}）` : ""}
              </span>
            ) : null}
            {lyricsFetchRunning(fetchProg) && fetchProg ? (
              <div className="progress-line lyrics-fetch-progress" aria-hidden="true">
                <div
                  style={{
                    width: `${
                      fetchProg.preparing
                        ? fetchProg.prepareTotal ? Math.round((fetchProg.prepareDone / fetchProg.prepareTotal) * 100) : 0
                        : fetchProg.total ? Math.round((fetchProg.done / fetchProg.total) * 100) : 0
                    }%`,
                  }}
                />
              </div>
            ) : null}
            <span className="hint">1曲ずつ、間を空けて送ります（1曲に 0.3〜1 秒ほど）。再生はそのまま続けられます。</span>
          </div>
        </section>

        <section>
          <h3>ショートカット（アプリ内）</h3>
          <ShortcutEditor />
        </section>

        <section>
          <h3>グローバルホットキー</h3>
          <HotkeyEditor />
        </section>

        <section>
          <h3>ウィンドウ</h3>
          <div className="field">
            <span className="flabel">×ボタンを押したとき</span>
            <span className="seg">
              <button className={s.closeToTray ? "on" : ""} onClick={() => updateSettings((x) => ({ ...x, closeToTray: true }))}>
                タスクトレイへ
              </button>
              <button className={!s.closeToTray ? "on" : ""} onClick={() => updateSettings((x) => ({ ...x, closeToTray: false }))}>
                完全に終了
              </button>
            </span>
            <span className="hint">タスクトレイのアイコンを右クリックすると「終了」を選べます。</span>
          </div>
        </section>

        <section>
          <h3>設定のバックアップ</h3>
          <div className="field">
            <span className="flabel">書き出し・読み込み</span>
            <span className="row">
              <button className="btn" onClick={() => api.settingsExport().then((r) => report(r, "export")).catch((e) => notify({ level: "error", error: e }))}>
                書き出す…
              </button>
              <button
                className="btn"
                onClick={async () => {
                  const ok = await confirmDialog(
                    "設定を読み込む",
                    "テーマ・背景・UI 設定・ショートカットは読み込んだ内容で置き換わります。プレイリストは追加し、お気に入りは印を足します。「歌詞をネットから取る」は、読み込んだ内容にかかわらずオフになります（使うときは、この画面でオンにしてください）。",
                    "読み込むファイルを選ぶ",
                  );
                  if (ok) api.settingsImport().then((r) => report(r, "import")).catch((e) => notify({ level: "error", error: e }));
                }}
              >
                読み込む…
              </button>
            </span>
            <span className="hint">対象：テーマ・背景・UI 設定・ショートカット・プレイリスト・お気に入り・ライブラリ設定（1つの zip）。歌詞とスキンは入りません（着せ替えの選択は、読み込んでも今のまま）。「歌詞をネットから取る」は、読み込むと常にオフになります。</span>
          </div>
        </section>

        <section>
          <h3>情報</h3>
          <div className="field">
            <span className="flabel">バージョン</span>
            <span>PL Player {info?.version ?? ""}</span>
          </div>
          <div className="field">
            <span className="flabel">ライブラリ</span>
            <span>{info ? `${info.trackCount}曲` : ""}</span>
          </div>
          <div className="field">
            <span className="flabel">データの場所</span>
            <span className="path-text">{info?.dataDir}</span>
          </div>
        </section>
      </div>
    </div>
  );
}
