// v3.8.0：遅れを測る（仕様「同時再生と配信モード」2.3 ）。入口のボタン（出力のプルダウンと「詳細」のミキサー）と小窓。
// 流れ：押す → 小窓で「クリック音が配信・録画に乗るので、止めてから測ってください」と断り、録る所を見せる → 「測る」 →
//       出力ごとに「届くまでの遅れ」と「入れる値」（または「測れませんでした：理由」） → 「この値を入れる」で遅らせ量に入れる
// - 測るのは曲を止めているときだけ。流している間は入口も「測る」も押せず、理由を出す（吹き出しと文字）。測る途中で流し始めたら Rust がやめる
// - 小窓を閉じる（Esc・外を押す・閉じる）と、測っている途中ならやめる
// - 決まり（文・入れる値の出し方）は src\layout\latency.ts。測る・計算は Rust（commands\latency.rs・audio\latency）
import { useEffect, useId, useRef, useState } from "react";
import { api } from "../ipc/commands";
import type { LatencyReport, LatencySetup } from "../ipc/types";
import { notify } from "../stores/notices";
import { outputs } from "../stores/outputs";
import { playback } from "../stores/playback";
import { createStore, useStore } from "../stores/store";
import { Modal } from "../components/Modal";
import { Tip } from "../components/Tip";
import {
  ABORT_TEXT,
  applyItems,
  captureChoices,
  captureName,
  captureOptions,
  LATENCY_TEXT,
  selectedCapture,
  latencyBlocked,
  latencyRowModel,
  reportNotes,
  skipText,
} from "./latency";

const dialogOpen = createStore(false);

/** 選び直した録る所（出力先の名前ごと。アプリを開いている間だけ覚える） */
const picked: Record<string, string> = {};

/**
 * 測定の番号（測るたびに増やす。「やめる」はこの番号で指す。：「測る」の直後の「やめる」が先に着いても効く）。
 * 画面を読み込み直しても前より大きくなるよう、今の時刻（マイクロ秒ほど）から始める（メーターの番号と同じ作り）
 */
let runSeq = 0;
const nextRun = () => (runSeq = Math.max(runSeq + 1, Date.now() * 1000));

/**
 * 入口のボタン。押せないとき（流している・鳴らせる出力先が2つ未満）は aria-disabled にして、理由を吹き出しで出す。
 * `showReason` なら、理由をボタンの下に文字でも出す（プルダウンのように幅があるところ）
 */
export function LatencyButton(props: { onOpen?: () => void; showReason?: boolean; className?: string }) {
  const v = useStore(outputs);
  const pb = useStore(playback);
  const blocked = latencyBlocked(v, pb.status === "playing");
  const reasonId = useId();
  // 理由を文字でも出すとき（プルダウン）は、その見える文をボタンの aria-describedby で指し、吹き出しの読み上げ用の文は付けない
  // （同じ理由を2回読ませない）。文字で出さないとき（ミキサー）は、吹き出しの読み上げ用の文で伝える
  const visibleReason = props.showReason === true && blocked !== null;
  return (
    <>
      <Tip text={blocked} delay={300} className="inline" describe={blocked !== null && !visibleReason}>
        <button
          className={"btn " + (props.className ?? "")}
          aria-disabled={blocked ? true : undefined}
          aria-describedby={visibleReason ? reasonId : undefined}
          aria-haspopup="dialog"
          onClick={() => {
            if (blocked) return;
            props.onOpen?.();
            dialogOpen.set(true);
          }}
        >
          {LATENCY_TEXT.button}
        </button>
      </Tip>
      {visibleReason ? (
        <span id={reasonId} className="hint-text latency-reason">
          {blocked}
        </span>
      ) : null}
    </>
  );
}

type Phase = { kind: "loading" } | { kind: "ready" } | { kind: "measuring" } | { kind: "result"; report: LatencyReport };

function LatencyDialog(props: { onClose: () => void }) {
  const v = useStore(outputs);
  const pb = useStore(playback);
  const [setup, setSetup] = useState<LatencySetup | null>(null);
  const [phase, setPhase] = useState<Phase>({ kind: "loading" });
  const [choice, setChoice] = useState<Record<string, string>>({ ...picked });
  const [applied, setApplied] = useState<string | null>(null);
  const [busyApply, setBusyApply] = useState(false);
  // 閉じたあとに届いた結果は使わない
  const alive = useRef(true);
  const measuring = useRef(false);
  const run = useRef(0);
  const blocked = latencyBlocked(v, pb.status === "playing");

  const loadSetup = () =>
    api.latencySetup().then(
      (s) => {
        if (!alive.current) return;
        // 今の一覧に無い録る所の選び直し（そのデバイスを抜いた）は捨てて、自動に戻す
        for (const r of s.rows) {
          const k = picked[r.name];
          if (k !== undefined && selectedCapture(r.autoCapture, s.inputs, k) !== k) delete picked[r.name];
        }
        setChoice({ ...picked });
        setSetup(s);
        setPhase((p) => (p.kind === "loading" ? { kind: "ready" } : p));
      },
      (e) => {
        if (!alive.current) return;
        notify({ level: "error", error: e });
        setPhase({ kind: "ready" });
      },
    );

  useEffect(() => {
    alive.current = true;
    void loadSetup();
    return () => {
      alive.current = false;
      // 閉じたら、測っている途中ならやめる
      if (measuring.current) void api.latencyCancel(run.current).catch(() => {});
    };
  }, []); // eslint-disable-line

  const close = () => props.onClose();

  const measure = () => {
    if (blocked || !setup) return;
    const my = nextRun();
    run.current = my;
    measuring.current = true;
    setApplied(null);
    setPhase({ kind: "measuring" });
    api
      .latencyMeasure(my, captureChoices(setup.rows, choice, setup.inputs))
      .then(
        (report) => {
          if (!alive.current || my !== run.current) return;
          setPhase({ kind: "result", report });
        },
        (e) => {
          if (!alive.current || my !== run.current) return;
          notify({ level: "error", error: e });
          setPhase({ kind: "ready" });
        },
      )
      .finally(() => {
        if (my === run.current) measuring.current = false;
      });
  };

  const cancel = () => {
    void api.latencyCancel(run.current).catch(() => {});
  };

  const apply = (report: LatencyReport) => {
    const items = applyItems(report);
    if (items.length === 0 || busyApply) return;
    setBusyApply(true);
    api
      .latencyApply(items)
      .then(
        (done) => {
          if (!alive.current) return;
          const skipped = items.length - done.length;
          setApplied(
            done.length === 0
              ? "測ってから出力先のデバイスが変わったので、入れませんでした。測り直してください。"
              : `遅らせ量を入れました（${done.length} 個の出力先）。` + (skipped > 0 ? `デバイスが変わった ${skipped} 個には入れていません。` : ""),
          );
        },
        (e) => notify({ level: "error", error: e }),
      )
      .finally(() => alive.current && setBusyApply(false));
  };

  const pick = (name: string, value: string) => {
    picked[name] = value;
    setChoice((c) => ({ ...c, [name]: value }));
  };

  const intro = (
    <>
      <p className="latency-caution">{LATENCY_TEXT.caution}</p>
      <p className="hint-text latency-p">
        {LATENCY_TEXT.quiet}
        {LATENCY_TEXT.how}
      </p>
      <p className="hint-text latency-p">{LATENCY_TEXT.scope}</p>
      <p className="hint-text latency-p">{LATENCY_TEXT.cable}</p>
    </>
  );

  let body: any;
  let footer: any;
  if (phase.kind === "loading") {
    body = (
      <>
        {intro}
        <p className="latency-p" role="status">
          出力先を調べています…
        </p>
      </>
    );
    footer = (
      <button className="btn" onClick={close}>
        閉じる
      </button>
    );
  } else if (phase.kind === "ready") {
    body = (
      <>
        {intro}
        <div className="latency-rows" role="list" aria-label="測る出力先">
          {(setup?.rows ?? []).map((r) => (
            <div key={r.slot} className="latency-row" role="listitem">
              <div className="latency-name" title={r.name}>
                {r.name}
              </div>
              {r.measurable && setup ? (
                <label className="latency-field">
                  <span className="hint-text">録る所</span>
                  <select
                    className="input latency-select"
                    value={selectedCapture(r.autoCapture, setup.inputs, choice[r.name])}
                    onChange={(e) => pick(r.name, e.target.value)}
                    aria-label={`${r.name}の録る所`}
                  >
                    {captureOptions(r.autoCapture, setup.inputs).map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </label>
              ) : (
                <div className="hint-text">{skipText(r.status)}</div>
              )}
            </div>
          ))}
        </div>
        {setup === null ? (
          <p className="latency-blocked" role="status">
            出力先を調べられませんでした。閉じて、もう一度開いてください。
          </p>
        ) : null}
        {blocked ? (
          <p className="latency-blocked" role="status">
            {blocked}
          </p>
        ) : null}
      </>
    );
    footer = (
      <>
        <button className="btn" onClick={close}>
          キャンセル
        </button>
        <Tip text={blocked} delay={300} className="inline" describe={false}>
          <button className="btn primary" aria-disabled={blocked || setup === null ? true : undefined} onClick={measure} autoFocus>
            {LATENCY_TEXT.start}
          </button>
        </Tip>
      </>
    );
  } else if (phase.kind === "measuring") {
    body = (
      <p className="latency-p" role="status" aria-live="polite">
        {LATENCY_TEXT.measuring}
      </p>
    );
    footer = (
      <button className="btn" onClick={cancel} autoFocus>
        やめる
      </button>
    );
  } else {
    const rep = phase.report;
    const items = applyItems(rep);
    const notes = reportNotes(rep);
    body = (
      <>
        {rep.abort ? (
          <p className="latency-blocked" role="status">
            {ABORT_TEXT[rep.abort]}
          </p>
        ) : (
          <div className="latency-rows" role="list" aria-label="測った結果">
            {rep.rows.map((r) => {
              const m = latencyRowModel(r);
              return (
                <div key={r.slot} className="latency-row" role="listitem">
                  <div className="latency-name" title={m.name}>
                    {m.name}
                  </div>
                  {m.arrival ? (
                    <div className="latency-line">
                      <span className="latency-label">届くまでの遅れ</span>
                      <span>{m.arrival}</span>
                    </div>
                  ) : null}
                  {m.fill ? (
                    <div className="latency-line">
                      <span className="latency-label">入れる値</span>
                      <span className="latency-value">{m.fill}</span>
                    </div>
                  ) : null}
                  {r.measured ? <div className="hint-text">録った所：{captureName(r.capture, setup?.inputs ?? [])}</div> : null}
                  {m.note ? <div className={m.failed ? "latency-fail" : "hint-text"}>{m.note}</div> : null}
                </div>
              );
            })}
          </div>
        )}
        {notes.map((n) => (
          <p key={n} className="hint-text latency-p">
            {n}
          </p>
        ))}
        <p className="hint-text latency-p">{LATENCY_TEXT.scope}</p>
        {applied ? (
          <p className="latency-p" role="status" aria-live="polite">
            {applied}
          </p>
        ) : null}
      </>
    );
    const retryBlocked = blocked;
    footer = (
      <>
        <button className="btn" onClick={close}>
          閉じる
        </button>
        <Tip text={retryBlocked} delay={300} className="inline" describe={false}>
          <button
            className="btn"
            aria-disabled={retryBlocked ? true : undefined}
            onClick={() => {
              if (retryBlocked) return;
              setPhase({ kind: "ready" });
              void loadSetup();
            }}
          >
            {LATENCY_TEXT.retry}
          </button>
        </Tip>
        {items.length > 0 && !applied ? (
          <button className="btn primary" disabled={busyApply} onClick={() => apply(rep)} autoFocus>
            {LATENCY_TEXT.apply}
          </button>
        ) : null}
      </>
    );
  }

  return (
    <Modal small title={LATENCY_TEXT.title} onClose={close} footer={footer}>
      <div className="latency-body">{body}</div>
    </Modal>
  );
}

/** 小窓の置き場所（App の直下。プルダウンが閉じても残る） */
export function LatencyHost() {
  const open = useStore(dialogOpen);
  if (!open) return null;
  return <LatencyDialog onClose={() => dialogOpen.set(false)} />;
}
