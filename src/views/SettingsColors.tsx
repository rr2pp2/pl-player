// 設定の「着せ替え」の中の「色」の欄（v3.10.0）：スキンで変えられる色 15 個を、1つずつ手で選ぶ。
// - 色の選び窓は、ブラウザに元からある <input type="color">（外のライブラリは足さない）。値を文字で打つ欄は作らない
// - 動かしている間は当てるだけ（stores/skin.ts の previewColor）。選び窓を閉じたとき（change）・つまみを離したときに Rust へ保存する
// - 手の色は Dark・Light 共通の1組。重なり順は テーマの既定 < スキンのそのテーマの色 < 手の色（決まりは src\skins\tokens.ts）
// - 読みやすさの警告は、今のテーマで、手の色のせいで新しく届かなくなった組み合わせだけ（スキンの取り込みと同じ contrast.ts。止めない）
import { useEffect, useMemo, useRef, useState } from "react";
import { contrastWarningsOrError } from "../skins/contrast";
import { themeDefaults } from "../skins/themeDefaults";
import { COLOR_ROWS, currentColors, manualColorVars, pickedColor, skinThemeColors, splitColor, warningLabel, type ColorKind, type ColorSource } from "../skins/tokens";
import { settings } from "../stores/settings";
import { clearColors, colorPreview, commitColor, currentTheme, dropColorPreview, previewColor, skinState } from "../stores/skin";
import { useStore } from "../stores/store";
import { confirmDialog } from "../components/Modal";

/** 濃さのつまみの刻み（%）。保存する濃さは小数3桁（0.1% 単位。色の確かめと同じ）なので、どの値でもつまみの位置と表示がそろう */
const ALPHA_STEP_PCT = 0.1;

/** 濃さ（0〜1）→ つまみの値と表示（%。小数1桁）。Light の既定 0.045 は 4.5 */
const pctOf = (a: number) => Math.round(a * 1000) / 10;

/** 1つの色の行：名前・見本（選び窓）・値・濃さのつまみ（半透明の5色だけ）・「戻す」（手で選んだ色だけ押せる。手で変えたかどうかはこれで分かる） */
function ColorRow(props: { name: string; label: string; kind: ColorKind; value: string; source: ColorSource; disabled: boolean }) {
  const parts = splitColor(props.value) ?? { hex: "#000000", alpha: 1 };
  const ref = useRef<HTMLInputElement>(null);
  const [alphaLocal, setAlphaLocal] = useState<number | null>(null);
  const alpha = alphaLocal ?? parts.alpha;
  // 最新の値を、選び窓の change（React の onChange ではなく、選び窓を閉じたときの本来の change）から読む
  const latest = useRef({ alpha: parts.alpha, kind: props.kind, name: props.name });
  latest.current = { alpha, kind: props.kind, name: props.name };

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onChange = () => {
      const l = latest.current;
      const v = pickedColor(l.kind, el.value, l.alpha);
      if (v !== null) void commitColor(l.name, v);
    };
    el.addEventListener("change", onChange);
    return () => el.removeEventListener("change", onChange);
  }, []);

  const commitAlpha = () => {
    if (alphaLocal === null) return;
    const v = pickedColor(props.kind, parts.hex, alphaLocal);
    setAlphaLocal(null);
    if (v !== null) void commitColor(props.name, v);
  };

  return (
    <div className="field color-row" data-color={props.name}>
      <span className="flabel">{props.label}</span>
      <span className="row">
        <input
          ref={ref}
          type="color"
          className="color-swatch"
          aria-label={`${props.label}の色を選ぶ`}
          value={parts.hex}
          disabled={props.disabled}
          onChange={(e) => {
            // React の onChange は選び窓を動かしている間（input）。当てるだけで保存しない
            const v = pickedColor(props.kind, e.target.value, alpha);
            if (v !== null) previewColor(props.name, v);
          }}
          // 選び窓を取り消したなど、保存しないまま離れたときは、動かしていた色を捨てる
          onBlur={() => dropColorPreview(props.name)}
        />
        <span className="color-value">{parts.hex}</span>
        {props.kind === "alpha" ? (
          <span className="row" style={{ flexWrap: "nowrap", gap: 6 }}>
            <input
              type="range"
              className="range color-alpha"
              aria-label={`${props.label}の濃さ`}
              min={0}
              max={100}
              step={ALPHA_STEP_PCT}
              value={pctOf(alpha)}
              disabled={props.disabled}
              style={{ ["--pct" as any]: `${pctOf(alpha)}%` }}
              onChange={(e) => {
                const a = Number(e.target.value) / 100;
                setAlphaLocal(a);
                const v = pickedColor(props.kind, parts.hex, a);
                if (v !== null) previewColor(props.name, v);
              }}
              onPointerUp={commitAlpha}
              onKeyUp={commitAlpha}
              onBlur={commitAlpha}
            />
            <span className="color-alpha-value">濃さ {pctOf(alpha)}%</span>
          </span>
        ) : null}
        <button className="btn small-btn" disabled={props.disabled || props.source !== "manual"} onClick={() => void commitColor(props.name, null)}>
          戻す
        </button>
      </span>
    </div>
  );
}

export function ColorsField() {
  const s = useStore(settings);
  const st = useStore(skinState);
  const theme = useStore(currentTheme);
  const preview = useStore(colorPreview);
  const safe = !!st?.safeMode;
  const saved = s.skin?.colors ?? {};
  const savedCount = Object.keys(manualColorVars(saved).vars).length;
  const manual = { ...saved, ...preview };
  // 今のテーマの既定の色（画面の CSS が正）と、当てているスキンのそのテーマの色
  // 既定の色はビルドした CSS から読むので、8桁の16進などは readThemeColors が今までの形にそろえる。読めない色があれば知らせる
  const { colors: defaults, unreadable } = useMemo(() => themeDefaults(theme), [theme]);
  const skinColors = skinThemeColors(safe ? null : st?.skin?.appearance ?? null, theme);
  const shown = currentColors(defaults, skinColors, safe ? {} : manual);
  const { rows: warnings, error: warnError } = useMemo(() => {
    if (safe) return { rows: [], error: null };
    // 計算できなかったときは黙って0件にせず、理由を出す
    return contrastWarningsOrError({ ...defaults, ...skinColors }, manualColorVars(manual).vars);
    // 手の色・スキン・テーマが変わったときだけ計算し直す
  }, [safe, defaults, JSON.stringify(skinColors), JSON.stringify(manual)]);

  const clearAll = async () => {
    const ok = await confirmDialog(
      "手で選んだ色を全部戻す",
      `手で選んだ色（${savedCount} 個）を全部外します。どの色も、スキンの色か既定の色に戻ります。スキンと立ち絵は変わりません。元に戻せません。`,
      "全部戻す",
      true,
    );
    if (ok) await clearColors();
  };

  return (
    <div className="colors-block">
      <h4 className="skin-subhead">色</h4>
      <p className="hint-text">
        15 個の色を1つずつ選べます。選んだ色はすぐ画面に出ます。Dark・Light のどちらにも同じ色を当て、当てているスキンの色より優先します（スキンを替えても残ります）。「既定の見た目に戻す」で、手で選んだ色も全部外れます。
      </p>
      {safe ? <div className="skin-callout warn">既定の見た目で動いている間（セーフモード）は、手で選んだ色も当てません。選んだ色は残っていて、セーフモードを抜けると戻ります。</div> : null}
      {unreadable.length || warnError ? (
        <div className="skin-callout warn" role="status">
          読みやすさを計算できませんでした（{unreadable.length ? `読めない既定の色：${unreadable.map(warningLabel).join("・")}` : warnError}）。色を選ぶことはできます。
        </div>
      ) : null}
      {warnings.length ? (
        <div className="skin-callout warn" role="status">
          <div>
            今のテーマ（{theme === "light" ? "Light" : "Dark"}）で、手で選んだ色のせいで読みにくくなる組み合わせが {warnings.length} 件あります（本文 4.5:1・線 3:1 に届かない。背景の暗さ 45%・UI の不透明度 70% で計算）。色はそのまま使えます。
          </div>
          <ul>
            {warnings.slice(0, 5).map((x, i) => (
              <li key={i}>
                {x.scene}：{warningLabel(x.name)} {x.ratio.toFixed(2)}:1
              </li>
            ))}
            {warnings.length > 5 ? <li>ほか {warnings.length - 5} 件</li> : null}
          </ul>
        </div>
      ) : null}
      <div className="color-list" role="group" aria-label="手で選ぶ色">
        {COLOR_ROWS.map((r) => (
          <ColorRow key={r.name} name={r.name} label={r.label} kind={r.kind} value={shown[r.name].value} source={shown[r.name].source} disabled={safe} />
        ))}
      </div>
      <div className="field">
        <span className="flabel">手で選んだ色</span>
        <span className="row">
          <button className="btn" disabled={savedCount === 0} onClick={() => void clearAll()}>
            手で選んだ色を全部戻す
          </button>
        </span>
        <span className="hint">
          板の色（パネル）と重ねる面の色の濃さは、「表示」の「UI の不透明度」と「背景の暗さ」で決まります。半透明の5色（指を乗せたとき・選んだ行・区切り線・操作部の輪郭・入力欄）は、色の隣のつまみで濃さも選べます。
        </span>
      </div>
    </div>
  );
}
