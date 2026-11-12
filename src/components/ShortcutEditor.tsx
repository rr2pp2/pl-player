// ショートカットの変更（アプリ内）と、グローバルホットキーの個別設定・競合の表示。
import { useEffect, useState } from "react";
import { actionLabel, hotkeyErrorText } from "../messages/ja";
import { DEFAULT_SHORTCUTS, hotkeyStatus, settings, updateSettings } from "../stores/settings";
import { useStore } from "../stores/store";
import { comboFromEvent, comboLabel, HOTKEY_ACTIONS, SHORTCUT_ACTIONS } from "../shortcuts/shortcuts";

function KeyBox(props: { value: string; bad?: boolean; onChange: (v: string) => void; allowClear?: boolean; requireModifier?: boolean }) {
  const [capturing, setCapturing] = useState(false);
  const [hint, setHint] = useState("");
  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setCapturing(false);
        return;
      }
      if ((e.key === "Backspace" || e.key === "Delete") && props.allowClear) {
        props.onChange("");
        setCapturing(false);
        return;
      }
      const c = comboFromEvent(e);
      if (!c) return;
      if (props.requireModifier && !/^(Ctrl|Alt|Shift|Super)\+/.test(c) && !/^(F\d+|Media|Audio)/.test(c)) {
        setHint("Ctrl・Alt・Shift のどれかと組み合わせてください");
        return;
      }
      props.onChange(c);
      setHint("");
      setCapturing(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, props]);
  return (
    <span className="row">
      <button
        className={"key-box" + (capturing ? " capturing" : "") + (props.bad ? " bad" : "")}
        onClick={() => setCapturing((c) => !c)}
        onBlur={() => setCapturing(false)}
        data-capture-keys
      >
        {capturing ? "キーを押してください…" : comboLabel(props.value)}
      </button>
      {capturing && hint ? <span className="error-text">{hint}</span> : null}
    </span>
  );
}

export function ShortcutEditor() {
  const s = useStore(settings);
  const map = s.shortcuts;
  const dup = (action: string) => {
    const v = map[action];
    return !!v && SHORTCUT_ACTIONS.some((a) => a !== action && map[a] === v);
  };
  return (
    <>
      {SHORTCUT_ACTIONS.map((a) => (
        <div className="field" key={a}>
          <span className="flabel">{actionLabel[a]}</span>
          <span className="row">
            <KeyBox
              value={map[a] ?? ""}
              bad={dup(a)}
              onChange={(v) => updateSettings((x) => ({ ...x, shortcuts: { ...x.shortcuts, [a]: v } }))}
            />
            {dup(a) ? <span className="error-text">ほかの操作と同じキーです</span> : null}
          </span>
        </div>
      ))}
      <div className="row" style={{ marginTop: 8 }}>
        <button className="btn" onClick={() => updateSettings((x) => ({ ...x, shortcuts: { ...DEFAULT_SHORTCUTS } }))}>
          既定に戻す
        </button>
        <span className="path-text">検索欄などに文字を打っている間は効きません。</span>
      </div>
    </>
  );
}

export function HotkeyEditor() {
  const s = useStore(settings);
  const st = useStore(hotkeyStatus);
  const statusOf = (a: string) => st.find((h) => h.action === a);
  return (
    <>
      <p className="path-text" style={{ marginTop: 0 }}>
        アプリが裏にあっても効くキーです。既定では何も割り当てていません。ほかのアプリと重ならないよう、Ctrl・Alt・Shift と組み合わせてください。
        修飾キー無しのキーや Ctrl+C などを割り当てると、ほかのアプリでそのキーが効かなくなります。
        消すときは、枠を押してから Delete キーを押します。
      </p>
      {HOTKEY_ACTIONS.map((a) => {
        const h = statusOf(a);
        const bad = !!h && !h.registered && !!s.hotkeys[a];
        return (
          <div className="field" key={a}>
            <span className="flabel">{actionLabel[a]}</span>
            <span className="row">
              <KeyBox
                value={s.hotkeys[a] ?? ""}
                bad={bad}
                allowClear
                requireModifier
                onChange={(v) =>
                  updateSettings((x) => {
                    const hk = { ...x.hotkeys };
                    if (v) hk[a] = v;
                    else delete hk[a];
                    return { ...x, hotkeys: hk };
                  })
                }
              />
              {bad ? <span className="error-text">{hotkeyErrorText(h!.error)}</span> : null}
              {h && h.registered && s.hotkeys[a] ? <span className="ok-text">登録済み</span> : null}
            </span>
          </div>
        );
      })}
    </>
  );
}
