// エラー種別 → 日本語の文。言い回しはここ1か所で直す（Rust 側では日本語の文を作らない）。
import type {
  AppError,
  ErrorCode,
  InboxReport,
  LegacyRereadProgress,
  LyricsFetchOne,
  LyricsFetchProgress,
  Notice,
  SkinNotice,
  SkinParts,
} from "../ipc/types";

export const errorText: Record<ErrorCode, string> = {
  file_not_found: "ファイルが見つかりません。移動・削除されたか、ドライブが外れている可能性があります。",
  access_denied: "ファイルを開く権限がありません。読み取り専用か、ほかのアプリが使用中の可能性があります。",
  unsupported_format: "この形式には対応していません。",
  tag_read_failed: "タグ（曲名などの情報）を読み込めませんでした。ファイル名で表示しています。",
  tag_write_failed: "タグを保存できませんでした。元のファイルは変更していません。",
  device_disconnected: "再生デバイスが切断されました。",
  playback_failed: "この曲を再生できませんでした。",
  database: "ライブラリのデータベースでエラーが起きました。",
  lyrics_format_invalid: "歌詞の内容を読み取れませんでした。内容を確認してください。",
  lyrics_encoding_unsupported: "歌詞ファイルの文字コードを読めませんでした。UTF-8 で保存し直してください。",
  lyrics_online_disabled: "「歌詞をネットから取る」がオフなので、通信しませんでした。設定の「歌詞をネットから取る」でオンにしてください。",
  lyrics_online_busy: "歌詞をネットから取っている途中です。終わってから、もう一度お試しください。",
  network_path_unsupported: "ネットワーク上の場所（\\\\ で始まる場所・ネットワークドライブ）は使えません。この PC のドライブにある場所を選んでください。",
  // 着せ替え（v3.2.0）。取り込みで断ったときは、何も入れていない
  skin_zip_invalid: "スキンの zip が壊れているか、読めない形です（分割・zip64・知らない圧縮方式・中身の破損など）。作り直した zip でお試しください。",
  skin_zip_encrypted: "パスワード付き（暗号化）の zip は取り込めません。",
  skin_too_large: "スキンが大きすぎます（zip 25MB・展開後 80MB・ファイル 64 個・画像1つ 15MB まで）。",
  skin_path_unsafe: "スキンの zip に、使えない名前のファイルがあります（フォルダの外を指す名前・リンク・大文字小文字だけ違う名前・英数字と . _ - 以外の文字など）。",
  skin_file_not_allowed: "スキンの zip に、入れられないファイルがあります（入れられるのは skin.json・preview と、icons・app・character の中の画像だけ。skin.json が使っていない画像も入れられません）。skin.json は zip のいちばん上か、zip の中のただ1つのフォルダの中に置いてください。",
  skin_executable: "スキンの zip に、実行ファイルかスクリプトが入っています。取り込めません。",
  skin_svg_not_allowed: "SVG の画像はこの版では使えません（PNG・WebP・GIF を使ってください）。",
  skin_css_not_allowed: "CSS のファイルはこの版では使えません。色と文字は skin.json の決まった項目で指定してください。",
  skin_font_not_allowed: "フォントのファイルはこの版では入れられません。PC にあるフォントの名前だけを skin.json に書けます。",
  skin_manifest_invalid: "skin.json を読めません（JSON の誤り・必須の項目が無い・値の形が違う・同じ項目が2つある・名前に見えない書式の文字がある など）。",
  skin_schema_unsupported: "この版のアプリでは読めないスキンの形式です（format・schemaVersion）。",
  skin_placement_unsupported: "この版で使える立ち絵の置き方は right-rail（右側に専用の幅を取る）だけです。",
  skin_asset_missing: "skin.json が指している画像が見つかりません（名前の大文字小文字も同じにしてください）。",
  skin_image_invalid: "画像を最後まで読めませんでした（壊れている・拡張子と中身が違う・アプリのアイコンが動く画像 など）。",
  skin_image_limit: "画像が大きすぎます（縦横 4096px・コマ数 120・長さ 90 秒まで）。",
  skin_id_exists: "同じ id のスキンが入っています。上書きはしません。入っているほうを消してから取り込み直してください。",
  skin_not_found: "そのスキンは入っていません。",
  skin_builtin_readonly: "既定のスキンは消したり書き出したりできません。",
  skin_apply_failed: "スキンを読めなかったので、既定の見た目に戻しました。",
  skin_import_expired: "取り込みの確認が古くなりました。もう一度 zip を選んでください。",
  // 手で選ぶ色（v3.10.0）。断ったときは何も変えていない
  skin_color_invalid: "その色は使えません（色の欄の 15 個だけ、#rrggbb などの決まった形だけを受けます）。何も変えていません。",
  // 立ち絵（v3.9.0。画像から追加する立ち絵）。画像の中身で断るときは、スキンの取り込みと同じ skin_image_invalid・skin_image_limit の文
  avatar_image_format: "立ち絵に使える画像は PNG・WebP・GIF（動く画像も）だけです。",
  avatar_too_large: "画像のファイルが大きすぎます（1つ 15MB まで）。",
  avatar_name_invalid: "その名前は使えません（1〜40 文字。見えない書式の文字・制御文字・空白だけの名前は使えません）。",
  avatar_limit: "登録できる立ち絵は 30 体までです。使わない立ち絵を削除してから作ってください。",
  avatar_not_found: "その立ち絵はありません（削除したか、取り込んだスキンに立ち絵がありません）。",
  avatar_broken: "立ち絵のファイルを読めません（消えた・壊れた など）。削除して作り直してください。",
  avatar_pick_expired: "選んだ画像の確認が古くなりました。もう一度画像を選んでください。",
  // 曲をごみ箱へ移す（v3.3.0）。断ったときは、その曲のファイルもライブラリも変えていない
  track_not_in_library: "その曲はライブラリにありません（もう外したか、一覧が古い可能性があります）。何も移していません。",
  trash_not_allowed: "ライブラリの曲ファイルとして確かめられなかったので、ごみ箱へ移しませんでした（音声ファイルでない・フォルダ・リンク）。",
  trash_unavailable: "ごみ箱の無い場所（USB メモリ・CD など、またはごみ箱を使わない設定のドライブ）にある曲です。ごみ箱へ移すと完全に消えてしまうので、移していません。",
  file_in_use: "ほかのアプリがこの曲のファイルを使っているので、ごみ箱へ移せませんでした。そのアプリを閉じてから、もう一度お試しください。",
  trash_too_many: "一度にごみ箱へ移せるのは 1,000 曲までです。選ぶ曲を減らしてから、もう一度お試しください。何も移していません。",
  // 古い文字コードのタグの読み直し（v3.5.0）。断ったときは何も変えていない
  legacy_reread_busy: "古い文字コードのタグを読み直している途中です。終わってから、もう一度お試しください。",
  library_import_busy: "曲の取り込み・再走査の途中なので、古い文字コードのタグの読み直しを始めませんでした。終わってから、もう一度お試しください。",
  // タグの編集画面の保存（v3.6.0）
  track_removed_before_save: "この曲はライブラリから外されたので、保存しませんでした（曲ファイルは変えていません）。",
  // 同時再生（v3.8.0）。断ったときは何も変えていない
  output_invalid: "出力先を変えられませんでした（出力先は3つまで・同じデバイスは1つの出力先にだけ入れられます）。",
  other: "処理を完了できませんでした。",
};

export function errorMessage(e: unknown): { text: string; detail: string } {
  const err = e as Partial<AppError> | string | undefined;
  if (err && typeof err === "object" && err.code && err.code in errorText) {
    return { text: errorText[err.code as ErrorCode], detail: err.detail ?? "" };
  }
  return { text: errorText.other, detail: typeof err === "string" ? err : JSON.stringify(err ?? "") };
}

export function noticeText(n: Notice): { text: string; detail: string } {
  if (n.level === "error") {
    return { text: errorText[n.code as ErrorCode] ?? errorText.other, detail: n.detail };
  }
  switch (n.code) {
    // v3.8.0：3.7.0 までの「切断されたので既定のデバイスで続けます」（device_fallback）は無くした。
    // 切断された出力先は「切断中」になり、戻ったら output_returned で知らせる（既定のデバイスへは移らない。仕様 2.5）
    // 同時再生（v3.8.0）
    case "output_returned":
      return { text: `${n.from || "出力先"}が戻りました。`, detail: "" };
    case "outputs_all_lost":
      return { text: "オンの出力先が全部使えなくなったので、一時停止しました。戻っても自動では再生しません。", detail: "" };
    case "sync_unavailable":
      return { text: "この PC では出力どうしの時刻をそろえられないので、ずれの自動の直しを止めました（音は出ます）。", detail: "" };
    case "outputs_imported":
      return { text: "出力先の設定を読み込みました。一時停止して、出力先は全部オフにしています。", detail: "" };
    case "import_busy":
      return { text: "取り込みの途中です。終わってから、もう一度お試しください。", detail: "" };
    case "import_refused_legacy_reread":
      return { text: "古い文字コードのタグを読み直している途中なので、取り込み・再走査をしませんでした。終わってから、もう一度お試しください。", detail: "" };
    default:
      return { text: n.code, detail: n.detail };
  }
}

export const actionLabel: Record<string, string> = {
  playPause: "再生 / 一時停止",
  next: "次の曲",
  previous: "前の曲",
  seekForward: "早送り（5秒）",
  seekBackward: "巻き戻し（5秒）",
  volumeUp: "音量を上げる",
  volumeDown: "音量を下げる",
  mute: "ミュート",
  shuffle: "シャッフル",
  repeat: "リピート",
  toggleLyrics: "歌詞を開く／閉じる",
  toggleLyricsRow: "流れる歌詞の列を出す／隠す",
  toggleDetail: "詳細表示の切替",
  toggleOutput1: "出力1 のオン／オフ",
  toggleOutput2: "出力2 のオン／オフ",
  toggleOutput3: "出力3 のオン／オフ",
};

export const hotkeyErrorText = (err: string | null): string => {
  if (!err) return "";
  if (err.startsWith("duplicate:")) {
    const other = err.slice("duplicate:".length);
    return `「${actionLabel[other] ?? other}」と同じキーです。`;
  }
  return "このキーは他のアプリが使っています。別のキーを選んでください。";
};

// ---- 歌詞（v3） ----

/** 歌詞の警告コード（Rust 側 lyrics/lrc.rs の warn・lyrics/resolver.rs）→ 文 */
export const lyricsWarningText: Record<string, string> = {
  timestamp_invalid: "一部の時刻を読み取れませんでした。原文はそのまま残しています。",
  timestamps_reordered: "時刻の順に並べ替えて表示しています。",
  offset_clamped: "時刻の補正（offset）で 0 秒より前になる行は、0 秒として扱います。",
  untimed_lines: "時刻の無い行は、末尾にまとめて表示しています。",
  timestamps_truncated: "1行の時刻が多すぎるため、一部の時刻を使っていません。原文はそのまま残しています。",
  embedded_tag_unreadable: "曲ファイルの歌詞タグを読み込めませんでした。",
  lyrics_too_large: "歌詞が大きすぎる（1MiB を超える）ため、表示していません。",
  lines_truncated: "歌詞の行が多すぎる（5,000行を超える）ため、先頭の 5,000行だけを表示しています。",
};

export const lyricsWarning = (code: string): string => lyricsWarningText[code] ?? code;

export const lyricsSourceLabel: Record<string, string> = {
  manual: "手動",
  external: "外部",
  embedded_tag: "曲のタグ",
};

/** 外部歌詞の取り込み結果を1行の文にする */
export function inboxReportText(r: InboxReport): { text: string; problems: string } {
  const parts: string[] = [];
  if (r.imported) parts.push(`${r.imported}件を取り込み`);
  if (r.manualKept) parts.push(`${r.manualKept}件は手動の歌詞が優先のため表示は変わりません`);
  const text = parts.length ? `外部歌詞：${parts.join("・")}` : "外部歌詞：取り込めるファイルはありませんでした";
  const bad: string[] = [];
  if (r.unknownTrack) bad.push(`曲 ID なし ${r.unknownTrack}件`);
  if (r.duplicate) bad.push(`.lrc と .txt の重複 ${r.duplicate}件`);
  if (r.encoding) bad.push(`文字コード ${r.encoding}件`);
  if (r.emptyOrTooLarge) bad.push(`空・大きすぎ ${r.emptyOrTooLarge}件`);
  if (r.keptInInbox) bad.push(`保存できず受け渡しフォルダに残したもの ${r.keptInInbox}件`);
  if (r.ignored) bad.push(`名前が取り決めに合わない・ファイルでないため読まなかったもの ${r.ignored}件`);
  if (r.linesTruncated) bad.push(`行が多すぎて先頭の 5,000行だけを表示するもの ${r.linesTruncated}件`);
  return { text, problems: bad.join("・") };
}

/** 流れる歌詞の列（v3.0.2）の状態の印 → 文（stores/lyricsTime.ts の RowStatus） */
export const lyricsRowText: Record<string, string> = {
  opening: "まもなく歌詞が始まります",
  interlude: "♪ 間奏",
  ended: "♪ 後奏",
  untimed: "時刻同期なし",
  none: "この曲には歌詞がありません",
  empty: "表示できる歌詞の行がありません",
  error: "歌詞を読み込めませんでした",
};

/** 時刻なしの歌詞のとき、列に出す案内。key は歌詞画面を開くショートカット（未設定なら「歌詞」ボタン） */
export const lyricsRowHint = (key: string): string => (key ? `全文歌詞は ${key} で表示できます` : "全文歌詞は「歌詞」ボタンで表示できます");

// ---- 歌詞をネットから取る（v3.1.0）。出すのは件数と状態だけ（歌詞の本文・LRCLIB 側の曲名は出さない） ----

/** 打ち切りの理由（Rust 側 lyrics/online.rs の aborted） */
export const lyricsFetchAbortText: Record<string, string> = {
  cancelled: "途中でやめました",
  disabled: "「歌詞をネットから取る」がオフになったので止めました",
  rate_limited_long_wait: "LRCLIB から長く待つよう求められたので止めました。しばらくしてからやり直してください",
  rate_limited_repeatedly: "LRCLIB から何度も待つよう求められたので止めました。しばらくしてからやり直してください",
  // too_many_errors は原因ごとに分ける（lyricsFetchAbortMessage）。これは原因が分からないときの文
  too_many_errors: "エラーが続いたので止めました",
  database: "ライブラリのデータベースを読めませんでした",
  network_init_failed: "通信の準備ができませんでした",
  internal_error: "途中で問題が起きたので止めました。もう一度お試しください",
};

/** 「エラーが続いたので止めました」の後ろに付ける、原因ごとの案内（最後のエラーの記号から） */
function errorAdvice(code: string | null): string {
  if (!code) return "";
  if (code === "timeout" || code === "network_error") return "ネットにつながっているか確かめてください";
  if (code === "tls_error") return "安全な接続を確かめられませんでした。PC の日付と時刻が合っているか確かめてください";
  if (code === "write_failed") return "ライブラリのデータベースに保存できませんでした";
  if (code === "bad_response" || code === "bad_lyrics_text" || code === "response_too_large") return "LRCLIB の応答を読めませんでした。しばらくしてからやり直してください";
  if (/^http_5\d\d$/.test(code)) return "LRCLIB が混み合っているか止まっています。しばらくしてからやり直してください";
  if (/^http_\d+$/.test(code)) return "LRCLIB が要求を受け付けませんでした。しばらくしてからやり直してください";
  return "";
}

/** 打ち切りの文。too_many_errors は最後のエラーの記号で原因を分ける（直し1回目） */
export function lyricsFetchAbortMessage(aborted: string, lastError: string | null): string {
  if (aborted !== "too_many_errors") return lyricsFetchAbortText[aborted] ?? aborted;
  const cause = lastError ? `（最後は「${lyricsFetchErrorText(lastError)}」）` : "";
  const advice = errorAdvice(lastError);
  return `エラーが続いたので止めました${cause}${advice ? "。" + advice : ""}`;
}

/** 1曲ぶんのエラーの記号 → 文 */
export function lyricsFetchErrorText(code: string | null): string {
  if (!code) return "";
  const fixed: Record<string, string> = {
    timeout: "応答がありませんでした",
    network_error: "ネットにつながりませんでした",
    tls_error: "安全な接続を確かめられませんでした",
    bad_response: "応答を読めませんでした",
    bad_lyrics_text: "応答の歌詞を読めませんでした",
    response_too_large: "応答が大きすぎました",
    not_allowed: "送り先が許可されていません",
    network_init_failed: "通信の準備ができませんでした",
  };
  if (fixed[code]) return fixed[code];
  if (lyricsFetchAbortText[code]) return lyricsFetchAbortText[code];
  if (/^http_\d+$/.test(code)) return `サーバーが ${code.slice(5)} を返しました`;
  return code;
}

/** 進み具合の1行（何曲中何曲・取れた・見つからない） */
export function lyricsFetchProgressText(p: LyricsFetchProgress): string {
  // 送る曲を選んでいる途中（曲名がファイル名と同じ曲は、曲のタグを読んで確かめるので時間がかかることがある）
  if (p.preparing) return `準備中 ${p.prepareTotal}曲中 ${p.prepareDone}曲`;
  const got = p.synced + p.plain;
  const parts = [`${p.total}曲中 ${p.done}曲`, `取れた ${got}`, `見つからない ${p.notFound}`];
  if (p.instrumental) parts.push(`歌の無い曲 ${p.instrumental}`);
  if (p.errors) parts.push(`エラー ${p.errors}`);
  return parts.join("・");
}

/** 終わったときの知らせ */
export function lyricsFetchDoneText(p: LyricsFetchProgress): { text: string; detail: string } {
  const got = p.synced + p.plain;
  const head = p.aborted ? "歌詞の取得を止めました" : "歌詞の取得が終わりました";
  const text = `${head}（${p.done}曲中 取れた ${got}曲・見つからない ${p.notFound}曲）`;
  const more: string[] = [];
  if (got) more.push(`時刻つき ${p.synced}・時刻なし ${p.plain}`);
  if (p.instrumental) more.push(`歌の無い曲（instrumental）${p.instrumental}`);
  if (p.tooLarge) more.push(`大きすぎて保存しなかった ${p.tooLarge}`);
  if (p.errors) more.push(`エラー ${p.errors}`);
  if (p.kept) more.push(`取り直せず今の歌詞を残した ${p.kept}`);
  if (p.hasTag) more.push(`曲のタグに歌詞があるので送らなかった ${p.hasTag}`);
  if (p.noMetadata) more.push(`曲名（タグ）かアーティスト名が空で送らなかった ${p.noMetadata}`);
  if (p.skippedKnown) more.push(`前に見つからなかったので聞き直さなかった ${p.skippedKnown}`);
  if (p.changedDuringRun) more.push(`途中でライブラリから外された・手動の歌詞が付いたので送らなかった ${p.changedDuringRun}`);
  if (p.aborted) more.push(lyricsFetchAbortMessage(p.aborted, p.lastError));
  return { text, detail: more.join("・") };
}

/** 1曲だけ取ったときの知らせ */
export function lyricsFetchOneText(r: LyricsFetchOne): { level: "info" | "error"; text: string } {
  switch (r.status) {
    case "synced":
      return { level: "info", text: "歌詞が見つかりました（時刻つき）" };
    case "plain":
      return { level: "info", text: "歌詞が見つかりました（時刻なし）" };
    case "instrumental":
      return { level: "info", text: "LRCLIB では歌の無い曲（instrumental）として登録されています" };
    case "not_found":
      return { level: "info", text: "LRCLIB に、この曲の歌詞は見つかりませんでした" };
    case "too_large":
      return { level: "info", text: "見つかった歌詞が大きすぎる（1MiB を超える）ため、保存しませんでした" };
    case "kept":
      return { level: "info", text: "新しい歌詞は見つからなかったので、今の歌詞を残しました" };
    case "no_metadata":
      return { level: "info", text: "曲のタグに曲名かアーティスト名が無いので探せません（ファイル名は送りません）。曲の情報を編集してください" };
    case "manual":
      return { level: "info", text: "手動の歌詞があるので探しませんでした" };
    case "stopped":
      return { level: "info", text: lyricsFetchAbortText[r.error ?? ""] ?? "止めました" };
    default:
      return { level: "error", text: `歌詞を取れませんでした（${lyricsFetchErrorText(r.error)}）` };
  }
}

// ---- 古い文字コードのタグの読み直し（v3.5.0）。出すのは件数だけ（曲名は出さない） ----

/** 押したときの確認の小窓 */
export const legacyRereadConfirm = {
  title: "古い文字コードのタグを読み直す",
  body:
    "ライブラリの全曲の曲名・アーティスト・アルバムなどを見直し、古い文字コード（Shift_JIS など）で化けたまま残っている名前を、今の決まりで直します。" +
    "直すのはこのアプリのライブラリの表示だけで、曲ファイルには書き込みません。編集画面で手で直した名前は書き換えません。" +
    "途中でやめられます（それまでに直した曲はそのままです）。",
  ok: "読み直す",
};

/** 進み具合の1行 */
export function legacyRereadProgressText(p: LegacyRereadProgress): string {
  return `${p.total}曲中 ${p.done}曲・直した ${p.fixed}曲`;
}

/** 終わったときの知らせ（設定画面の状態の行にも同じ文を出す） */
export function legacyRereadDoneText(p: LegacyRereadProgress): { level: "info" | "error"; text: string; detail: string } {
  const more: string[] = [];
  if (p.skippedManual) more.push(`編集画面で手で直した項目がある ${p.skippedManual}曲は、その項目を変えていません`);
  if (p.changedDuringRun) more.push(`読み直しの途中で名前が変わった・ライブラリから外された ${p.changedDuringRun}曲は、変えていません`);
  if (p.error) {
    more.unshift(errorText[p.error] ?? errorText.other);
    return {
      level: "error",
      text: `古い文字コードのタグの読み直しを、途中で止めました（${p.done}曲を見て、${p.fixed}曲を直しました。直した曲はそのままです）`,
      detail: more.join("・"),
    };
  }
  if (p.cancelled) {
    return { level: "info", text: `古い文字コードのタグの読み直しをやめました（${p.done}曲を見て、${p.fixed}曲を直しました）`, detail: more.join("・") };
  }
  const text = p.fixed
    ? `古い文字コードのタグを読み直しました。${p.fixed}曲の名前を直しました（${p.done}曲を見ました）`
    : `古い文字コードのタグを読み直しました。直せる名前はありませんでした（${p.done}曲を見ました）`;
  return { level: "info", text, detail: more.join("・") };
}

// ---- 着せ替え（v3.2.0） ----

/** 取り込みの警告（Rust 側 skins/manifest.rs の Warning.code）→ 文 */
export const skinWarningText: Record<string, string> = {
  unknown_field: "知らない項目は使いません",
  layout_token_ignored: "レイアウトの寸法と背景の設定は、スキンからは変えられません（使いません）",
  unknown_token_ignored: "知らない変数は使いません",
  min_app_version_newer: "このスキンは、もっと新しい版のアプリ向けです。一部が使われないことがあります",
  extensions_ignored: "あとの版のための項目（自作 CSS・フォント・音など）は、この版では読みません",
  hide_on_lyrics_forced: "立ち絵は歌詞画面では常に隠します",
  tray_playing_ignored: "再生中のトレイのアイコンは、この版では使いません",
};

export const skinWarning = (code: string): string => skinWarningText[code] ?? code;

/** 起動・2つ目の起動・壊れたスキンの知らせ */
export function skinNoticeText(n: SkinNotice): { level: "info" | "error"; text: string; detail: string } {
  switch (n.code) {
    case "broken_fallback":
      return {
        level: "error",
        text: `前に選んでいたスキン「${n.id ?? ""}」を読めなかったので、既定の見た目で起動しました。`,
        detail: n.error ? errorText[n.error] ?? "" : "",
      };
    case "safe_mode":
      return { level: "info", text: "既定の見た目で起動しました（セーフモード）。保存しているスキンには、設定の「着せ替え」から戻せます。", detail: "" };
    case "safe_mode_second":
      return { level: "info", text: "既定の見た目に切り替えました（--safe-mode）。保存しているスキンには、設定の「着せ替え」から戻せます。", detail: "" };
    case "avatar_fallback":
      return {
        level: "error",
        text: "使う立ち絵に選んでいた絵が見つからないか読めなかったので、「スキンの立ち絵」に戻しました。",
        detail: n.error ? errorText[n.error] ?? "" : "",
      };
    default:
      return { level: "info", text: n.code, detail: "" };
  }
}

/** スキンの部品の札（一覧・確認画面） */
export function skinPartLabels(p: SkinParts): string[] {
  const out: string[] = [];
  if (p.appearance) out.push("色・文字");
  if (p.icons > 0) out.push(`アイコン ${p.icons}`);
  if (p.application) out.push("アプリのアイコン");
  if (p.character) out.push("立ち絵");
  if (p.animated) out.push("動く画像");
  return out;
}

// ---- タグの編集画面の保存（v3.6.0） ----

/** 保存できたときの知らせ。曲ファイルには書いたが、その間に曲がライブラリから外された（Rust が null を返した）ときは、戻していないことを言う */
export function metadataSavedText(inLibrary: boolean): string {
  return inLibrary
    ? "タグを保存しました"
    : "タグを曲ファイルに保存しました。保存の途中でこの曲がライブラリから外されたので、ライブラリには戻していません。";
}

// ---- 年間まとめ（v3.6.0） ----

/** 数え始めた版（記録が無い・少ないときの知らせに出す） */
export const YEARLY_SINCE_VERSION = "3.6.0";

/** 時刻（ミリ秒）を、この PC の時刻の日付に（「2026年11月14日」） */
export function yearlyDate(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`;
}

/** 数えた期間（「2025年11月14日〜2026年11月14日」。365 日さかのぼったところから、画面を開いた時点まで） */
export function yearlyPeriodText(t: { since: number; until: number }): string {
  return `${yearlyDate(t.since)}〜${yearlyDate(t.until)}`;
}

/** 記録がまだ無いとき（期間の中に1回も無い）の一覧の文 */
export const yearlyEmptyText = `まだ再生の記録がありません。${YEARLY_SINCE_VERSION} から、再生した回数を数え始めました（それより前の再生は数えていません）。`;

/**
 * 記録が1年に満たないときの知らせ（数え始めたのが期間の始まりより後）。それ以外は null。
 * 記録が無いときは一覧の文（yearlyEmptyText）が同じことを言うので、ここでは出さない
 */
export function yearlyStartNote(t: { since: number; startedAt: number | null; ranked: number }): string | null {
  if (t.ranked === 0 || t.startedAt === null || t.startedAt <= t.since) return null;
  return `${YEARLY_SINCE_VERSION} から数え始めたので、${yearlyDate(t.startedAt)}より前の再生は入っていません。`;
}
