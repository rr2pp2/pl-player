// 小窓（名前の入力・確認・編集画面）。
import { useEffect, useRef, useState } from "react";
import { createStore, useStore } from "../stores/store";

export function Modal(props: { title: string; onClose: () => void; children: any; footer?: any; small?: boolean }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props.onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()} data-capture-keys>
      <div className={"modal" + (props.small ? " small" : "")} role="dialog" aria-modal="true" aria-label={props.title}>
        <div className="modal-head">{props.title}</div>
        <div className="modal-body">{props.children}</div>
        {props.footer ? <div className="modal-foot">{props.footer}</div> : null}
      </div>
    </div>
  );
}

// ---- 名前の入力・確認を、関数で呼べるようにする ----

type Dialog =
  | { kind: "prompt"; title: string; label: string; value: string; ok: string; resolve: (v: string | null) => void }
  | { kind: "confirm"; title: string; message: string; ok: string; danger?: boolean; resolve: (v: boolean) => void };

const dialogState = createStore<Dialog | null>(null);

export function promptText(title: string, label: string, value = "", ok = "OK"): Promise<string | null> {
  return new Promise((resolve) => dialogState.set({ kind: "prompt", title, label, value, ok, resolve }));
}

export function confirmDialog(title: string, message: string, ok = "OK", danger = false): Promise<boolean> {
  return new Promise((resolve) => dialogState.set({ kind: "confirm", title, message, ok, danger, resolve }));
}

function PromptBody(props: { d: Extract<Dialog, { kind: "prompt" }> }) {
  const [v, setV] = useState(props.d.value);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const done = (r: string | null) => {
    dialogState.set(null);
    props.d.resolve(r);
  };
  return (
    <Modal
      small
      title={props.d.title}
      onClose={() => done(null)}
      footer={
        <>
          <button className="btn" onClick={() => done(null)}>
            キャンセル
          </button>
          <button className="btn primary" disabled={!v.trim()} onClick={() => done(v.trim())}>
            {props.d.ok}
          </button>
        </>
      }
    >
      <label className="flabel" style={{ display: "block", marginBottom: 6, color: "var(--c-secondary)" }}>
        {props.d.label}
      </label>
      <input
        ref={ref}
        className="input"
        value={v}
        maxLength={200}
        onChange={(e) => setV(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && v.trim()) done(v.trim());
        }}
      />
    </Modal>
  );
}

export function DialogHost() {
  const d = useStore(dialogState);
  if (!d) return null;
  if (d.kind === "prompt") return <PromptBody key={d.title + d.value} d={d} />;
  const done = (r: boolean) => {
    dialogState.set(null);
    d.resolve(r);
  };
  return (
    <Modal
      small
      title={d.title}
      onClose={() => done(false)}
      footer={
        <>
          <button className="btn" onClick={() => done(false)}>
            キャンセル
          </button>
          <button className={"btn " + (d.danger ? "danger" : "primary")} onClick={() => done(true)} autoFocus>
            {d.ok}
          </button>
        </>
      }
    >
      <p style={{ margin: 0, whiteSpace: "pre-wrap" }}>{d.message}</p>
    </Modal>
  );
}
