// v3.11.1：短い名前を出す。「・」「／」の後ろに <wbr> を入れ、狭いときはそこで折り返す（区切り方は wrapParts.ts）。文字は変えない
import { Fragment } from "react";
import { wrapParts } from "./wrapParts";

export function WrapText(props: { text: string }) {
  return (
    <>
      {wrapParts(props.text).map((p, i) => (
        <Fragment key={i}>
          {i > 0 ? <wbr /> : null}
          {p}
        </Fragment>
      ))}
    </>
  );
}
