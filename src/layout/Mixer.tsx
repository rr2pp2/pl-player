// v3.8.0 段2-2：詳細（仕様の「配信モード」）のミキサー（仕様 6.2〜6.4。OBS の音声ミキサーに寄せる）。
// 出力先ごとに1行。左右のときも上下のときも同じ形の行を縦に並べる（3行まで。行の寸法は src\layout\detailLayout.ts の MIXER）。
//   上の段：アイコン・名前（長ければ「…」）・全体の音量に従わない印・状態（切断中／開けません／重複）・音割れの印・今の大きさの dB・オン/オフ・「…」
//   下の段：メーター（左右2本・色の帯・直前の最大の線・-60〜0 dB の目盛り）・音量つまみ・%
// - オン/オフ（スピーカーの印）はプルダウンのチェックと同じもの（2.4）。オフは消音の形にし、名前を薄い文字色に（opacity で薄くしない）
// - 「…」：遅らせ量（0〜1000 ミリ秒）・全体の音量に従う・デバイスの差し替え（既定のデバイスも）・アイコンの選び直し・外す（確認なし。6.3）
// - 見出しの行に「＋ 出力先を追加」（3つあるときは押せない。マウスを乗せると「出力先は3つまで」）。足した直後はオフ（Rust が決める）
// - 次の版のプラグインの入れ口（11章）は、場所だけ空けてボタンは出さない（行の右の並びに足せる形にしてある）
import { useEffect, useRef, useState } from "react";
import { api } from "../ipc/commands";
import type { OutputIcon, OutputSlotView } from "../ipc/types";
import { notify } from "../stores/notices";
import { devices, outputs, watchDevices } from "../stores/outputs";
import { useStore } from "../stores/store";
import { Tip } from "../components/Tip";
import { Popover } from "../components/Popover";
import { MeterBars, MeterReadout } from "../components/Meter";
import { IconMore, IconPlus, IconVolume, OutputIconView } from "../components/Icons";
import { OutputStatusText, OutputVolume } from "./OutputButton";
import { LatencyButton } from "./LatencyDialog";
import { canAddOutput, deviceChoices, OUTPUT_ICONS, registered, slotIcon, slotName, slotTitle, type DeviceChoice } from "./outputs";

const fail = (e: unknown) => notify({ level: "error", error: e });

/** デバイスの一覧（足す・差し替える）。ほかの出力先で使っているものは出さない */
function DeviceList(props: { slot: number | null; current?: string | null; onPick: (c: DeviceChoice) => void }) {
  const v = useStore(outputs);
  const list = useStore(devices);
  // 出ている間だけ、デバイスの変化の知らせで一覧を引き直す（閉じたら受け口を外す）
  useEffect(() => watchDevices(), []);
  const choices = deviceChoices(list, v, props.slot);
  return (
    <div className="output-device-list" role="group" aria-label="デバイス">
      {choices.map((c) => {
        const cur = props.slot !== null && (props.current ?? null) === c.id;
        return (
          <button key={c.id ?? "default"} className={"menu-item" + (cur ? " current" : "")} aria-current={cur || undefined} onClick={() => props.onPick(c)}>
            <span style={{ flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis" }} title={c.name}>
              {c.name}
              {c.isDefault ? "（今の既定）" : ""}
            </span>
            {cur ? <span className="output-current">使用中</span> : null}
          </button>
        );
      })}
      {choices.length <= 1 && list.length === 0 ? <div className="output-pop-empty">再生デバイスが見つかりません</div> : null}
    </div>
  );
}

/**
 * 遅らせ量（0〜1000 ミリ秒、1 ミリ秒刻み）。打ち終えて 0.3 秒で送る（Rust はその出力だけ入り直すので、打つたびには送らない）。
 * 入力欄にフォーカスがある間は、Rust から戻ってきた値で欄を書き換えない（打っている途中の桁を消さない。段2-2 検査1回目の指摘5）。
 * 欄の文字を値にそろえるのは、離れたとき・Enter のときと、ほかの所（別の操作）で値が変わったとき（フォーカスが無いとき）だけ
 */
function DelayField(props: { s: OutputSlotView }) {
  const [text, setText] = useState(String(props.s.delayMs));
  const timer = useRef<number | undefined>(undefined);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (document.activeElement === input.current) return;
    setText(String(props.s.delayMs));
  }, [props.s.delayMs]);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const send = (raw: string, now: boolean) => {
    window.clearTimeout(timer.current);
    const n = Number(raw);
    if (raw.trim() === "" || !Number.isFinite(n)) {
      if (now) setText(String(props.s.delayMs));
      return;
    }
    const ms = Math.max(0, Math.min(1000, Math.round(n)));
    const go = () => {
      if (now) setText(String(ms));
      if (ms !== props.s.delayMs) api.outputSetDelay(props.s.slot, ms).catch(fail);
    };
    if (now) go();
    else timer.current = window.setTimeout(go, 300);
  };
  const id = `delay-${props.s.slot}`;
  return (
    <div className="mixer-menu-field">
      <label htmlFor={id}>遅らせ量</label>
      <span className="row" style={{ gap: 6 }}>
        <input
          ref={input}
          id={id}
          className="input mixer-delay-input"
          type="number"
          min={0}
          max={1000}
          step={1}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            send(e.target.value, false);
          }}
          onBlur={(e) => send(e.target.value, true)}
          onKeyDown={(e) => e.key === "Enter" && send((e.target as HTMLInputElement).value, true)}
        />
        <span>ミリ秒（0〜1000）</span>
      </span>
      <span className="hint-text">再生デバイスどうしの鳴るタイミングを合わせるためのものです（映像に合わせるものではありません）。変えると、この出力先だけ一瞬途切れます。</span>
    </div>
  );
}

/** 行の「…」の中身 */
function SlotMenu(props: { s: OutputSlotView; onDone: () => void }) {
  const s = props.s;
  const [devicesOpen, setDevicesOpen] = useState(false);
  const name = slotName(s);
  return (
    <div className="mixer-menu">
      <div className="mixer-menu-title" title={slotTitle(s)}>
        {name}
      </div>
      <DelayField s={s} />
      <label className="row check-long mixer-menu-field" style={{ gap: 6 }}>
        <input type="checkbox" checked={s.followMaster} onChange={(e) => api.outputSetFollow(s.slot, e.target.checked).catch(fail)} />
        <span>
          全体の音量に従う
          <span className="hint-text">外すと、この出力先は再生バーの音量とミュートで変わりません（詳細の表示のときだけ効きます）。</span>
        </span>
      </label>
      <div className="mixer-menu-field">
        <span>アイコン</span>
        <span className="seg mixer-icon-seg" role="radiogroup" aria-label="アイコン">
          <button role="radio" aria-checked={s.icon === null} className={s.icon === null ? "on" : ""} onClick={() => api.outputSetIcon(s.slot, null).catch(fail)} title="機器の種類から自動で決める">
            自動
          </button>
          {OUTPUT_ICONS.map((ic) => (
            <button
              key={ic.id}
              role="radio"
              aria-checked={s.icon === ic.id}
              aria-label={ic.label}
              title={ic.label}
              className={s.icon === ic.id ? "on" : ""}
              onClick={() => api.outputSetIcon(s.slot, ic.id as OutputIcon).catch(fail)}
            >
              <span className="output-icon" aria-hidden="true">
                <OutputIconView icon={ic.id} />
              </span>
            </button>
          ))}
        </span>
      </div>
      <div className="mixer-menu-field">
        <button className="btn" aria-expanded={devicesOpen} onClick={() => setDevicesOpen((o) => !o)}>
          デバイスを差し替える…
        </button>
        {devicesOpen ? (
          <DeviceList
            slot={s.slot}
            current={s.deviceId}
            onPick={(c) => {
              setDevicesOpen(false);
              api.outputSetDevice(s.slot, c.id).catch(fail);
            }}
          />
        ) : null}
      </div>
      <div className="mixer-menu-field">
        <button
          className="btn danger"
          onClick={() => {
            props.onDone();
            api.outputRemove(s.slot).catch(fail);
          }}
          title="すぐに外します。音量・遅らせ量などは覚えておき、同じデバイスを足すと戻ります"
        >
          この出力先を外す
        </button>
      </div>
    </div>
  );
}

function MixerRow(props: { s: OutputSlotView }) {
  const s = props.s;
  const name = slotName(s);
  const [menu, setMenu] = useState(false);
  const more = useRef<HTMLButtonElement>(null);
  const closeMenu = (focusBack: boolean) => {
    setMenu(false);
    if (focusBack) more.current?.focus();
  };
  // 全体の音量に従わない印：詳細でだけ効くので、ミキサー（詳細でだけ出す）ではいつも出す
  const independent = !s.followMaster;
  return (
    <div className={"mixer-row" + (s.on ? "" : " off")} role="group" aria-label={name}>
      <div className="mixer-row-top">
        <span className="output-icon" aria-hidden="true">
          <OutputIconView icon={slotIcon(s)} />
        </span>
        <span className="output-name" title={slotTitle(s)}>
          {name}
        </span>
        {independent ? (
          <Tip text="全体の音量に従いません（再生バーの音量とミュートで変わりません）" delay={300} className="inline">
            <span className="mixer-badge" tabIndex={0}>
              個別
            </span>
          </Tip>
        ) : null}
        <OutputStatusText slot={s} />
        {/* 音割れの印と dB の数字は右に寄せる（間を埋める要素は置かない。間が1つ増えるぶん名前が縮むため） */}
        <span className="mixer-readout">
          <MeterReadout slot={s.slot} number />
        </span>
      </div>
      <div className="mixer-row-buttons">
        <button
          className={"icon-btn small" + (s.on ? "" : " is-off")}
          onClick={() => api.outputSetOn(s.slot, !s.on).catch(fail)}
          // 読み上げの名前は固定し、オンかどうかは aria-pressed だけで伝える（名前と状態を二重に読ませない。段2-2 検査1回目の指摘7）。
          // 吹き出し（title）は仕様 2.4 の「この出力をオフにする」
          aria-label={`${name}：オン`}
          aria-pressed={s.on}
          title={s.on ? "この出力をオフにする" : "この出力をオンにする"}
        >
          <IconVolume muted={!s.on} level={1} />
        </button>
        <button
          ref={more}
          className={"icon-btn small" + (menu ? " on" : "")}
          onClick={() => setMenu((o) => !o)}
          aria-label={`${name}の設定`}
          aria-haspopup="dialog"
          aria-expanded={menu}
          title="遅らせ量・全体の音量に従う・デバイスの差し替え・アイコン・外す"
        >
          <IconMore />
        </button>
      </div>
      <div className="mixer-row-bottom">
        <MeterBars slot={s.slot} scale label={`${name}のメーター（-60〜0 dB）`} />
        <OutputVolume slot={s} label={`${name}の音量`} />
      </div>
      {menu ? (
        <Popover anchor={more} place="below" label={`${name}の設定`} onClose={closeMenu} className="mixer-menu-pop">
          <SlotMenu s={s} onDone={() => closeMenu(false)} />
        </Popover>
      ) : null}
    </div>
  );
}

function AddButton() {
  const v = useStore(outputs);
  const can = canAddOutput(v);
  const [open, setOpen] = useState(false);
  const btn = useRef<HTMLButtonElement>(null);
  const close = (focusBack: boolean) => {
    setOpen(false);
    if (focusBack) btn.current?.focus();
  };
  return (
    <>
      <Tip text={can ? null : "出力先は3つまで"} delay={300} className="inline" describe={!can}>
        <button
          ref={btn}
          className="btn mixer-add"
          aria-disabled={can ? undefined : true}
          aria-haspopup="dialog"
          aria-expanded={open}
          onClick={() => can && setOpen((o) => !o)}
        >
          <IconPlus /> 出力先を追加
        </button>
      </Tip>
      {open ? (
        <Popover anchor={btn} place="below" label="出力先を追加" onClose={close} className="mixer-add-pop">
          <div className="mixer-menu-title">足すデバイス</div>
          <div className="hint-text" style={{ padding: "0 10px 6px" }}>
            足した出力先はオフで入ります（知らない機器から急に音が出ないように）。オンにすると鳴ります。
          </div>
          <DeviceList
            slot={null}
            onPick={(c) => {
              close(true);
              api.outputAdd(c.id).catch(fail);
            }}
          />
        </Popover>
      ) : null}
    </>
  );
}

export function Mixer(props: { scrolls: boolean }) {
  const v = useStore(outputs);
  const all = registered(v);
  return (
    <section className={"mixer panel" + (props.scrolls ? " scrolls" : "")} aria-label="出力のミキサー">
      <div className="mixer-head">
        <h2 className="mixer-title">出力</h2>
        {/* v3.8.0 段3：遅れを測る。押せないとき（流している・鳴らせる出力先が2つ未満）は、理由を吹き出しで出す */}
        <LatencyButton />
        <AddButton />
      </div>
      <div className="mixer-rows">
        {all.length === 0 ? (
          <div className="mixer-empty">出力先がありません。「＋ 出力先を追加」で足せます。</div>
        ) : (
          all.map((s) => <MixerRow key={s.slot} s={s} />)
        )}
      </div>
    </section>
  );
}
