//! 同時再生（同じ曲を最大3つの再生デバイスへ、ずれ 10ms 以内で流す）を作る前に、作者の実機で測るためのテスト。
//! 本体の音の処理（`src\audio\`）は使わず・変えず、cpal・rubato・windows（QPC）だけで直接デバイスを開く。
//! 仕様の「作る前に技術で決めること」13.1 の「作者の実機で測るもの」1〜6 に当てる。
//!
//! # 4つの計測（どれも `#[ignore]`。実際のデバイスから音が出るので、手で走らせる）
//!
//! | 計測 | 測るもの（13.1） | 鳴る音 | 既定の長さ |
//! |---|---|---|---|
//! | `drift_probe` | 1 時計の速さ・位置の揺れ・のこぎり形／2 L の値／6 頭打ちの回数 | 無音 | 3600 秒 |
//! | `click_probe` | 3 録音でずれを測る／5 途中参加の差（録音は作者が別に、マイクや OBS で） | 短いクリック（1台目 1kHz・2台目 3kHz・3台目 5kHz。-12 dBFS） | 2秒おきに10回 |
//! | `loopback_click_probe` | 3・5 を PC の中で：鳴らしている出力をループバックで（と、指定した録音デバイスも）録り、ずれを自動で出す。**機器の中・USB の先の遅れは測れない**（下の「何を測っていて、何を測っていないか」） | `click_probe` と同じクリック（-30 dBFS） | 2秒おきに10回 |
//! | `slip_probe` | 4 `rubato::Slip` の試聴 | 1kHz の正弦波（-20 dBFS） | 60 秒 |
//!
//! # 走らせ方（`src-tauri` のフォルダで）
//!
//! まずデバイスの名前を知る。`PL_PROBE_DEVICES` を付けずに走らせると、一覧を出して失敗する（音は出ない）。
//!
//! ```text
//! cargo test --features test-support --test sync_probe -- --ignored --exact drift_probe --nocapture
//! ```
//!
//! PowerShell：
//!
//! ```text
//! $env:PL_PROBE_DEVICES = "スピーカー|ヘッドホン"
//! cargo test --features test-support --test sync_probe -- --ignored --exact drift_probe --nocapture
//! cargo test --features test-support --test sync_probe -- --ignored --exact click_probe --nocapture
//! $env:PL_PROBE_DEVICES = "ヘッドホン"
//! cargo test --features test-support --test sync_probe -- --ignored --exact slip_probe --nocapture
//! Remove-Item Env:PL_PROBE_DEVICES   # 終わったら消す（ほかの環境変数も同じ）
//! ```
//!
//! Git Bash：`PL_PROBE_DEVICES='スピーカー|ヘッドホン' cargo test --features test-support --test sync_probe -- --ignored --exact drift_probe --nocapture`
//!
//! `loopback_click_probe`（マイクも OBS も使わない）。`PL_PROBE_DEVICES` を付けずに走らせると、再生デバイスと録音デバイスの一覧を出して止まる（音は出ない）。
//! 仮想ケーブルは、再生デバイス側（入口）と録音デバイス側（出口）で名前が別なので、一覧で確かめる：
//!
//! ```text
//! $env:PL_PROBE_DEVICES = "ヘッドホン|仮想ケーブルの入口"
//! $env:PL_PROBE_CAPTURE = "仮想ケーブルの出口"   # 付けなければループバックだけ
//! cargo test --features test-support --test sync_probe -- --ignored --exact loopback_click_probe --nocapture
//! ```
//!
//! 同じ計測を2つ同時に走らせない（同じデバイスを取り合う）。
//!
//! 途中で止めるときは Ctrl+C。止めたときに残るもの：
//! - 全部：`callbacks.csv`（1秒ごとに書き出し）
//! - drift_probe：最初の要約は1分後、そのあと `PL_PROBE_SUMMARY_EVERY_SEC` ごとに `drift_summary.txt` と残りの CSV を書き直す。
//!   止めても直前の要約は残る。**1分より前に止めると要約は無く、`callbacks.csv` だけ**（止めたあとに計算し直す手段は無いので、測り直す）
//! - click_probe・loopback_click_probe・slip_probe：始める前に設定とデバイスだけの「途中の要約」を書く。全体の要約（回数・音切れ・ずれの平均など）は最後まで流したときだけ。
//!   狙った時刻は `click_targets.csv`（随時）、滑らせた所は `slip_chunks.csv`（約1秒ごと）に残る。
//!   loopback_click_probe は、録ったパケットが `capture_packets.csv`（1秒ごと）、見つけたクリックが `loopback_clicks.csv`（調べるたびに書き直し）に残る
//!
//! 数の指定が数でない・範囲の外（NaN・無限・負・大きすぎる値）なら、デバイスを開く前に止まる。
//! 長さ・時刻の上限は 7日（604800 秒）。音の大きさ（`PL_PROBE_CLICK_DB`・`PL_PROBE_SINE_DB`）は -80〜-6 dBFS で、
//! -6 を超える指定（-20 を 20 と打ったなど）は切り詰めずに止める。
//!
//! # 環境変数
//!
//! 「使う計測」の lb は `loopback_click_probe`。
//!
//! | 名前 | 使う計測 | 既定 | 意味 |
//! |---|---|---|---|
//! | `PL_PROBE_DEVICES` | 全部 | （無し＝一覧を出して失敗） | 鳴らすデバイス（lb は 1〜3 個、click は 2〜3 個）。`\|`（縦棒1本。表の中なので \ を付けて書いている）で区切って並べる（並べた順が デバイス1・2・3。1が基準）。1つずつ「`default`（Windows の既定）」「`#番号`（一覧の番号）」「ID」「名前」「名前の一部（1つに決まるとき）」のどれか。同じデバイスを2回は書けない |
//! | `PL_PROBE_OUT` | 全部 | `%USERPROFILE%\Documents\pl-player-probe` | 記録を置く親のフォルダ。この下に `<日時>`（ローカル時刻 `YYYYMMDD-HHMMSS`。同じ秒にあれば `-2` を付ける）のフォルダを作る。**リポジトリの外に置く**（要約にデバイスの名前と ID が入る。公開リポジトリに混ざらないよう、リポジトリの中を指すと止まる） |
//! | `PL_PROBE_SECS` | drift・slip・click・lb | drift 3600／slip 60／click・lb（無し） | 流す長さ（秒）。click・lb では「全体の長さ」で、`PL_PROBE_CLICK_AT_SEC` が無いとき、0 秒・半分・最後の3か所で10回ずつ鳴らす（3600 なら 0・30分・60分） |
//! | `PL_PROBE_SKIP_MS` | 全部 | 200 | 開いてからこの間のコールバックは、当てはめに使わない（13.1 の「開いて約200ms は捨てる」） |
//! | `PL_PROBE_SUMMARY_EVERY_SEC` | drift | 600 | 途中の要約を書き直す間隔（10〜86400。最初の1回は1分後） |
//! | `PL_PROBE_CLICK_INTERVAL_MS` | click・lb | 2000 | クリックの間隔（lb は `PL_PROBE_CAPTURE_WINDOW_MS` の2倍＋クリックの長さ＋10ms 以上） |
//! | `PL_PROBE_CLICK_COUNT` | click・lb | 10 | 1か所で鳴らす回数（途中参加なら 20 にすると 13.1 の「20回」） |
//! | `PL_PROBE_CLICK_AT_SEC` | click・lb | （無し＝`0`） | 10回ずつ鳴らし始める時刻（秒）を `,` で区切って。例 `0,1800,3600` |
//! | `PL_PROBE_CLICK_MS` | click・lb | 5 | クリック1回の長さ（前後 0.5ms を弱める） |
//! | `PL_PROBE_CLICK_DB` | click・lb | click -12／lb -30 | クリックの大きさ（dBFS。-80〜-6）。lb は耳で聞かないので小さめ。ほかの音（曲など）が大きくて「小さい」で取れないときは上げる |
//! | `PL_PROBE_CLICK_LEAD_MS` | click・lb | 3000 | 開いてから最初のクリックまでの間 |
//! | `PL_PROBE_SUBTRACT_L` | click・lb | 1 | 1＝`playback` から L を引いて狙う／0＝引かない（13.1 の 3。暫定は「引く」） |
//! | `PL_PROBE_AIM` | click・lb | `fit` | `fit`＝直近 `PL_PROBE_FIT_WINDOW_MS` の直線の当てはめで狙う／`raw`＝その回の `playback` 1点で狙う |
//! | `PL_PROBE_FIT_WINDOW_MS` | click・lb | 3000 | 当てはめに使う直近の長さ |
//! | `PL_PROBE_JOIN` | click・lb | 0 | 1＝途中参加：2台目だけ、毎回クリックの前に開き直す（ほかは開いたまま。lb のループバックは2台目も開いたまま録る） |
//! | `PL_PROBE_JOIN_OPEN_BEFORE_MS` | click・lb | 600 | 途中参加で、クリックの何 ms 前に2台目を開き直すか |
//! | `PL_PROBE_GATE_MS` | click・lb | 80 | 途中参加の「開始時刻＝今＋約80ms」。開いて `PL_PROBE_SKIP_MS` ＋ これより前のクリックは狙わない |
//! | `PL_PROBE_CAPTURE` | lb | （無し＝ループバックだけ） | 鳴らしている出力のループバックに加えて録る録音デバイス（4個まで。並べた順が cap1・cap2…）。指定の仕方は `PL_PROBE_DEVICES` と同じ（`default` は Windows の既定の録音デバイス、`#番号` は録音デバイスの一覧の番号）。OBS が実際に受け取る側（仮想ケーブルの出口など）を録るため |
//! | `PL_PROBE_CAPTURE_WINDOW_MS` | lb | 200 | 狙った時刻の前後どこまでを探すか（20〜500）。録音デバイスの遅れがこれを超えると「音が無い」になるので広げる |
//! | `PL_PROBE_SLIP_RATIO` | slip | 1.0001 | `Slip` の倍率（固定。0.99〜1.01） |
//! | `PL_PROBE_SLIP_ALTERNATE_SEC` | slip | 0 | 0＝ずっと `Slip` を通す／正の数＝その秒数ごとに「通す」「通さない」を交互に（最初は通す） |
//! | `PL_PROBE_SINE_HZ` | slip | 1000 | 正弦波の高さ |
//! | `PL_PROBE_SINE_DB` | slip | -20 | 正弦波の大きさ（dBFS。-80〜-6） |
//!
//! デバイスは今のアプリと同じ開き方（共有モード・既定の形式・既定のバッファ）で開く。lb の録る側も共有モード・既定の形式（ミックスの形式）・既定のバッファ。
//! ループバックは、cpal 0.18.2 が出力デバイスに入力の流れを開くと `AUDCLNT_STREAMFLAGS_LOOPBACK` を付けることを使う（`src\host\wasapi\device.rs` の
//! `build_input_stream_raw_inner`。Windows 10 1703 以降は、ループバックでも知らせ（イベント）で動く）。新しいクレートは無い。外へは通信しない。
//!
//! # できるファイル（`<親>\<日時>\`。CSV は ASCII だけ。デバイスの名前は要約の txt に書く）
//!
//! ## 全部の計測：`callbacks.csv`（コールバック1回で1行）
//!
//! | 列 | 意味 |
//! |---|---|
//! | `dev` | デバイス番号（1から。`PL_PROBE_DEVICES` に並べた順） |
//! | `open` | そのデバイスを開いた回（途中参加で開き直すと増える。ほかは 1） |
//! | `seq` | その回のコールバックの通し番号（0から） |
//! | `frames` | このコールバックで書いたフレーム数 |
//! | `written_before` | このコールバックより前に書いた総フレーム数（cpal が `playback` の計算に使う値と同じ） |
//! | `callback_ns` | cpal の `callback`（QPC を ns にしたもの。WASAPI の `GetPosition` が返した時刻） |
//! | `playback_ns` | cpal の `playback`（`callback`＋書いて未消化の量＋L） |
//! | `qpc_entry_ns` | コールバックに入ったときに自分で読んだ QPC（ns。cpal と同じく 100ns 単位に丸める） |
//! | `stalled` | 1＝`playback` が前の回より進んでいない（cpal が前に戻らないように止めた＝頭打ち） |
//! | `pb_minus_cb_us` | `playback − callback`（µs）。最初の回は L そのもの |
//! | `entry_minus_cb_us` | `qpc_entry − callback`（µs）。cpal の時刻とコールバックの実際の時刻の差 |
//!
//! ## drift_probe
//!
//! - `drift_summary.txt`：デバイスごとのレート・チャンネル数・L・L が途中で変わった疑い・時計の速さ（ppm）・当てはめの残りの揺れ・
//!   頭打ちの回数・基準（デバイス1）との ppm の差と「1時間で何 ms ずれるか」。**まずこれを見る**
//! - `drift_residuals_dev<N>.csv`：当てはめの残りを時刻順に。列は `seq,t_s,written_before,residual_us,stalled`。
//!   `t_s`（開いてからの秒）を横、`residual_us` を縦にしてグラフにすると、周期ののこぎり形（階段）があるかが見える
//!
//! 時計の速さは「書いた総フレーム数（`written_before`）」を横、「`playback − L`」を縦にした直線の傾きから出す
//! （傾き＝1フレームが鳴るのにかかる QPC の ns。＋ppm は名目のレートより速く進む）。頭打ちの回と、開いて `PL_PROBE_SKIP_MS` までは外す。
//!
//! 見るところ：基準との差の「1時間で何 ms」（10ms を超えるなら時計のずれの直しが要る）・残りの揺れ（13.1 の「不感帯 ±1ms」
//! 「基準を替える揺れ 2ms」を決める）・開いて2秒までの揺れ（基準の選び方）・L がデバイスごとに違うか・頭打ちが多いか。
//!
//! ## click_probe
//!
//! - `click_targets.csv`：何回目のクリックを、どの QPC 時刻に狙ったか（デバイスごと）。列は
//!   `dev,open,click,status,target_ns,target_rel_ms,start_frame,aimed_ns,aim_err_us,fit_minus_raw_us,fit_points,l_us,subtract_l,aim,decided_ns`
//!   - `status`：`ok`＝鳴らした／`late`＝狙った時刻のフレームをもう書いていた（鳴らしていない）／`too_soon`＝途中参加で、開いてすぐ過ぎて狙わなかった
//!   - `target_rel_ms`：最初のクリックの狙いからの ms（録音の上で何回目かを数える目安）
//!   - `aimed_ns`・`aim_err_us`：実際に選んだフレームが鳴ると見込んだ QPC 時刻と、狙いとの差（フレームの刻みの丸めで ±10µs 程度）
//!   - `fit_minus_raw_us`：当てはめで狙った場合と、その回の `playback` 1点で狙った場合の差（どちらで鳴らしたかは `aim`）
//! - `click_summary.txt`：設定・デバイスごとの L・開き直しの記録・測り方
//!
//! 見るところ：作者が別に録音したもの（Audacity。左右別の入力が無ければスマホで、2つのスピーカーをマイクから同じ距離に）で、
//! 同じ回の 1kHz と 3kHz の立ち上がりの差を読む。それが実際のずれ（`aim_err_us` の差はふつう無視できる大きさ）。
//! 開始・30分・60分で差が変わらないか。L がデバイスで違うなら `PL_PROBE_SUBTRACT_L=1` と `0` の両方で録る。
//!
//! ## loopback_click_probe
//!
//! 鳴らし方は click_probe と同じ（`callbacks.csv`・`click_targets.csv` も同じものができる）。そのうえで、録る流れを開く：
//! `lb1`〜`lb3`＝鳴らしている各デバイスのループバック（その高さだけ探す）、`cap1`〜＝`PL_PROBE_CAPTURE` の録音デバイス（全部の高さを探す）。
//!
//! 何を測っていて、何を測っていないか：
//! - **ループバックの時刻は、Windows の音のエンジンがその出力デバイスへ渡した音に付けた QPC 時刻**（cpal の入力の `capture`。
//!   WASAPI の `GetBuffer` が返す、パケットの最初のフレームの時刻）。**USB・HDMI・Bluetooth の先と、機器の中の遅れは入らない**（耳に届く時刻ではない）
//! - **録音デバイスの時刻は、その入力に入ってきた時刻。**仮想ケーブルなら、入口（再生デバイス側）から出口（録音デバイス側）へ渡る遅れと、入力の遅れも入る（OBS が受け取る側に近い）
//! - 立ち上がりは、クリックの最初の標本の時刻。2ms のハン窓でその高さの大きさを出し、半分の大きさを横切る左右の中点から、クリックの形で逆算する。
//!   作った波形での確かめ：雑音だけなら約 10µs 以内、ほかのデバイスのクリックが 1.25ms 以内に重なると最大約 0.06ms 偏る
//! - 取れなかった回は数えるだけで、差の平均には入れない（狙った時刻の前後 `PL_PROBE_CAPTURE_WINDOW_MS` の窓だけを見る。
//!   その高さの音が無い・雑音（窓の中の中央値）より 20dB 大きくない・窓の端で切れている・長さが違う・隣の高さ（＋1kHz）も一緒に上がる・同じ高さの山がほかにもある、は断る）
//!
//! - `loopback_summary.txt`：設定・鳴らしたデバイス（L など）・録った流れ（形式・パケットの数・途切れ・時刻の飛び・取りこぼし）・
//!   流れと高さごとに「狙いとの差」「基準との差」「lb1 との差」の平均・揺れ（標準偏差）・最小・最大、取れなかった回数（理由ごと）、回ごとの表。**まずこれを見る**
//! - `loopback_clicks.csv`：流れ・高さ・回ごとに1行。列は
//!   `stream,dev,open,click,status,target_ns,target_rel_ms,onset_ns,onset_minus_target_us,minus_base_us,minus_lb1_us,aimed_minus_target_us,peak_dbfs,snr_db,width_ms`
//!   - `stream`：`lb1`〜（ループバック）・`cap1`〜（録音デバイス）。名前は要約の txt
//!   - `dev`：探した高さのデバイス番号（1＝1kHz・2＝3kHz・3＝5kHz）。`open`：途中参加の2台目は開き直しの回（click＋1）
//!   - `status`：`ok`／`not_played`（その回は鳴らしていない。click_targets.csv の status が ok でない）／`not_captured`（前後が録れていない）／
//!     `gap`（前後で録音が途切れた）／`no_signal`／`weak`／`truncated`／`wrong_shape`／`not_tone`／`ambiguous`
//!   - `onset_minus_target_us`：狙いとの差。`minus_base_us`：基準との差（lb は lb1 の 1kHz、cap はその中の 1kHz）。`minus_lb1_us`：cap だけ、lb1 の 1kHz との差。どれも＋は遅い
//!   - `aimed_minus_target_us`：選んだフレームの丸め（click_targets.csv の aim_err_us と同じ）
//! - `capture_packets.csv`：録ったパケットごとに `stream,pos,stored_before,frames,capture_ns,callback_ns,qpc_entry_ns,jump_us,break`。
//!   `jump_us` は前のパケットから見込んだ時刻との差、`break` が 1 の所で途切れた（取りこぼしか、1ms を超える時刻の飛び）
//!
//! 見るところ：基準との差の平均と揺れ（10ms 以内か。開始・30分・60分で変わらないか）。途中参加なら2台目の基準との差（開き直しごと）。
//! 録音デバイスを付けたなら、その中の基準との差（OBS が受け取る音の上でのずれ）と lb1 との差（OBS が受け取るまでの遅れ）。
//!
//! ## slip_probe
//!
//! - `slip_chunks.csv`：作った塊（1024 フレームを入力）ごとに `chunk,t_s,mode,in_frames,out_frames,slipped`。
//!   `slipped` が 0 でない行が「フレームを足した／抜いた」所。`t_s` は鳴り始めからの秒の目安
//! - `slip_summary.txt`：設定・滑った回数・音切れ（輪が空になった回数）
//!
//! 見るところ：耳で聞いて、`slipped` の時刻に音のにごりやプチッという音が分かるか。交互（`PL_PROBE_SLIP_ALTERNATE_SEC`）にすると比べやすい。
//!
//! # 音を出さずに確かめる部分
//!
//! 計算（直線の当てはめ・ppm・揺れ・頭打ちの判定・クリックの波形・狙う時刻・デバイスの選び方・フォルダの名前、
//! lb の「録った音からクリックの立ち上がりを探す」「高さで見分ける」「パケットのつながり」「差の集計」）は純粋な関数に分け、
//! このファイルの末尾の、`#[ignore]` の付かないテストで確かめる（`cargo test --features test-support --test sync_probe`。`#[ignore]` は走らない）。

use std::collections::{BTreeMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use pl_player_lib::audio::devices::{self, DeviceInfo};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Adjustable, FixedAsync, Resampler, Slip};

// ======================================================================
// 純粋な関数（音を出さずに確かめる）
// ======================================================================

/// QPC の値を ns にする。cpal（`stream.rs` の `now`）と同じく、先に 100ns 単位へ丸める。
fn qpc_to_ns(counter: i64, freq: i64) -> u64 {
    if counter <= 0 || freq <= 0 {
        return 0;
    }
    let units_100ns = counter as u128 * 10_000_000 / freq as u128;
    (units_100ns * 100).min(u64::MAX as u128) as u64
}

/// 直線 y = intercept + slope · x
#[derive(Clone, Copy, Debug, PartialEq)]
struct Line {
    slope: f64,
    intercept: f64,
}

impl Line {
    fn at(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }
    fn x_for(&self, y: f64) -> f64 {
        (y - self.intercept) / self.slope
    }
    /// 1点と傾きから
    fn through(x: f64, y: f64, slope: f64) -> Self {
        Self { slope, intercept: y - slope * x }
    }
}

/// 最小二乗で直線を当てる（平均を引いてから計算するので、大きな値でも桁が落ちにくい）。2点未満・x が全部同じなら None。
fn fit_line<I>(pts: I) -> Option<Line>
where
    I: Iterator<Item = (f64, f64)> + Clone,
{
    let (mut n, mut sx, mut sy) = (0f64, 0f64, 0f64);
    for (x, y) in pts.clone() {
        n += 1.0;
        sx += x;
        sy += y;
    }
    if n < 2.0 {
        return None;
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut sxx, mut sxy) = (0f64, 0f64);
    for (x, y) in pts {
        let dx = x - mx;
        sxx += dx * dx;
        sxy += dx * (y - my);
    }
    if sxx <= 0.0 {
        return None;
    }
    let slope = sxy / sxx;
    Some(Line { slope, intercept: my - slope * mx })
}

/// 当てはめの残り（y − 直線）の揺れ
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ResidualStats {
    n: usize,
    /// 標準偏差（y の単位）
    std: f64,
    /// 絶対値の最大
    max_abs: f64,
}

fn residual_stats<I>(pts: I, line: &Line) -> ResidualStats
where
    I: Iterator<Item = (f64, f64)>,
{
    let (mut n, mut s, mut ss, mut max_abs) = (0usize, 0f64, 0f64, 0f64);
    for (x, y) in pts {
        let r = y - line.at(x);
        n += 1;
        s += r;
        ss += r * r;
        max_abs = max_abs.max(r.abs());
    }
    if n == 0 {
        return ResidualStats::default();
    }
    let mean = s / n as f64;
    let var = (ss / n as f64 - mean * mean).max(0.0);
    ResidualStats { n, std: var.sqrt(), max_abs }
}

/// 傾き（1フレームが鳴るのにかかる ns）から、名目のレートに対する ppm。＋は名目より速い。
fn ppm_from_slope(slope_ns_per_frame: f64, nominal_rate: u32) -> f64 {
    (1e9 / (slope_ns_per_frame * nominal_rate as f64) - 1.0) * 1e6
}

/// ppm の差を「1時間で何 ms ずれるか」に（1ppm × 3600秒 = 3.6ms）
fn drift_ms_per_hour(ppm: f64) -> f64 {
    ppm * 3.6
}

/// 頭打ち：cpal は `playback` を前に戻さないように、前の回の値で止める（`host\mod.rs` の `monotonic_output_callback`）。
/// 前の回より進んでいなければ頭打ちとみなす。
fn is_stalled(prev_playback_ns: Option<u64>, playback_ns: u64) -> bool {
    matches!(prev_playback_ns, Some(p) if playback_ns <= p)
}

fn db_to_amp(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// 秒で指定する長さ・時刻の上限（7日）。これより大きい値は ns に直すと桁があふれうるので断る
const MAX_PROBE_SECS: f64 = 604_800.0;

/// 数の指定を確かめる。数でない（NaN）・無限・範囲の外は断る
fn check_range(name: &str, v: f64, min: f64, max: f64) -> Result<f64, String> {
    if !v.is_finite() || v < min || v > max {
        Err(format!("{name}={v} は {min}〜{max} の中で"))
    } else {
        Ok(v)
    }
}

/// 鳴らす音の大きさの上限（dBFS）。-20 を 20 と打ち間違えても、フルスケールで鳴らさないため
const MAX_LEVEL_DB: f64 = -6.0;
const MIN_LEVEL_DB: f64 = -80.0;

/// 音の大きさ（dBFS）を倍率に。上限を超える・数でない値は断る（切り詰めて鳴らさない）
fn level_amp(name: &str, db: f64) -> Result<f32, String> {
    check_range(name, db, MIN_LEVEL_DB, MAX_LEVEL_DB).map(db_to_amp)
}

/// `child` が `root` の中か（Windows のパスは大文字小文字を区別しないので、小文字にして比べる。どちらも絶対パスで渡す）
fn path_is_inside(child: &Path, root: &Path) -> bool {
    let norm = |p: &Path| -> Vec<String> {
        p.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect()
    };
    let (c, r) = (norm(child), norm(root));
    !r.is_empty() && c.len() >= r.len() && c[..r.len()] == r[..]
}

/// クリック1回の波形（モノラル）。正弦波を `dur_ms` だけ、前後 `fade_ms` を半分の余弦で弱める。
fn click_burst(freq_hz: f64, rate: u32, dur_ms: f64, fade_ms: f64, amp: f32) -> Vec<f32> {
    let n = ((dur_ms / 1000.0) * rate as f64).round().max(1.0) as usize;
    let fade = (((fade_ms / 1000.0) * rate as f64).round() as usize).min(n / 2);
    (0..n)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let edge = i.min(n - 1 - i);
            let g = if fade > 0 && edge < fade {
                0.5 - 0.5 * (std::f64::consts::PI * (edge as f64 + 0.5) / fade as f64).cos()
            } else {
                1.0
            };
            (amp as f64 * g * (2.0 * std::f64::consts::PI * freq_hz * t).sin()) as f32
        })
        .collect()
}

/// デバイス番号（1から）ごとのクリックの高さ
fn click_freq_for(dev_no: u8) -> f64 {
    match dev_no {
        1 => 1000.0,
        2 => 3000.0,
        _ => 5000.0,
    }
}

/// 狙う時刻の一覧（QPC の ns）。`burst_starts_s` の各時刻から `count` 回、`interval_ms` おき。重なりは1つにまとめる。
fn click_targets(base_ns: u64, burst_starts_s: &[f64], count: usize, interval_ms: f64) -> Vec<u64> {
    let mut v: Vec<u64> = burst_starts_s
        .iter()
        .flat_map(|&s| {
            (0..count).map(move |i| base_ns + ((s * 1e9) + (i as f64 * interval_ms * 1e6)).round().max(0.0) as u64)
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// click の「何秒の所で10回鳴らすか」。明示が無く、全体の長さだけあれば 0・半分・最後
fn click_burst_starts(at_sec: Option<&str>, total_secs: Option<f64>) -> Result<Vec<f64>, String> {
    if let Some(s) = at_sec {
        let mut v = vec![];
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let x: f64 = part.parse().map_err(|_| format!("PL_PROBE_CLICK_AT_SEC の「{part}」が数ではない"))?;
            check_range("PL_PROBE_CLICK_AT_SEC の1つ", x, 0.0, MAX_PROBE_SECS)?;
            v.push(x);
        }
        if v.is_empty() {
            return Err("PL_PROBE_CLICK_AT_SEC が空".into());
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        return Ok(v);
    }
    if let Some(t) = total_secs {
        check_range("PL_PROBE_SECS", t, 0.0, MAX_PROBE_SECS)?;
    }
    Ok(match total_secs {
        Some(t) if t > 0.0 => vec![0.0, t / 2.0, t],
        _ => vec![0.0],
    })
}

/// 時刻（直線の y と同じ基準）に鳴るフレーム
fn aim_frame(line: &Line, target_rel_ns: f64) -> i64 {
    line.x_for(target_rel_ns).round() as i64
}

/// `t` のフレームで、`k` から始まるクリックの何番目の標本を書くか（範囲外は None）
fn burst_index(frame: u64, start_frame: i64, burst_len: usize) -> Option<usize> {
    let pos = frame as i64 - start_frame;
    if pos >= 0 && (pos as usize) < burst_len {
        Some(pos as usize)
    } else {
        None
    }
}

/// 直近の点だけを持つ当てはめの窓。容量は先に取るので、コールバックの中で増やさない。
struct FitWindow {
    pts: VecDeque<(f64, f64, u64)>,
    cap: usize,
    window_ns: u64,
}

impl FitWindow {
    fn new(cap: usize, window_ns: u64) -> Self {
        Self { pts: VecDeque::with_capacity(cap), cap, window_ns }
    }
    fn push(&mut self, x: f64, y: f64, t_ns: u64) {
        if self.pts.len() == self.cap {
            self.pts.pop_front();
        }
        self.pts.push_back((x, y, t_ns));
        while let Some(&(_, _, t0)) = self.pts.front() {
            if t_ns.saturating_sub(t0) > self.window_ns && self.pts.len() > 2 {
                self.pts.pop_front();
            } else {
                break;
            }
        }
    }
    fn len(&self) -> usize {
        self.pts.len()
    }
    fn fit(&self) -> Option<Line> {
        fit_line(self.pts.iter().map(|&(x, y, _)| (x, y)))
    }
    fn latest(&self) -> Option<(f64, f64)> {
        self.pts.back().map(|&(x, y, _)| (x, y))
    }
}

/// 位相がつながる正弦波（インターリーブで全チャンネルに同じ値）
struct SineGen {
    phase: f64,
    step: f64,
    amp: f32,
}

impl SineGen {
    fn new(freq_hz: f64, rate: u32, amp: f32) -> Self {
        Self { phase: 0.0, step: freq_hz / rate as f64, amp }
    }
    fn fill(&mut self, out: &mut [f32], channels: usize) {
        for frame in out.chunks_mut(channels.max(1)) {
            let v = self.amp * (2.0 * std::f64::consts::PI * self.phase).sin() as f32;
            frame.fill(v);
            self.phase = (self.phase + self.step).fract();
        }
    }
}

/// slip_probe の区間：true＝`Slip` を通す。`alternate_sec` が 0 以下ならずっと通す
fn slip_on_at(out_frame: u64, rate: u32, alternate_sec: f64) -> bool {
    if alternate_sec <= 0.0 {
        return true;
    }
    let t = out_frame as f64 / rate as f64;
    ((t / alternate_sec).floor() as u64) % 2 == 0
}

/// `PL_PROBE_DEVICES` からデバイスを選ぶ。返すのは `list` の添字（並べた順）
fn select_devices(list: &[DeviceInfo], spec: &str) -> Result<Vec<usize>, String> {
    select_from(list, spec, "再生", "PL_PROBE_DEVICES")
}

/// デバイスの一覧から選ぶ（`kind` は「再生」「録音」、`env` は空のときに知らせる環境変数の名前）
fn select_from(list: &[DeviceInfo], spec: &str, kind: &str, env: &str) -> Result<Vec<usize>, String> {
    let mut out: Vec<usize> = vec![];
    for entry in spec.split('|').map(str::trim).filter(|e| !e.is_empty()) {
        let idx = if entry.eq_ignore_ascii_case("default") {
            list.iter().position(|d| d.is_default).ok_or(format!("Windows の既定の{kind}デバイスが無い"))?
        } else if let Some(n) = entry.strip_prefix('#') {
            let i: usize = n.trim().parse().map_err(|_| format!("「{entry}」：# のあとが番号ではない"))?;
            if i >= list.len() {
                return Err(format!("「{entry}」：番号は 0〜{} の中で", list.len().saturating_sub(1)));
            }
            i
        } else if let Some(i) = list.iter().position(|d| d.id == entry) {
            i
        } else if let Some(i) = list.iter().position(|d| d.name == entry) {
            i
        } else {
            let low = entry.to_lowercase();
            let hits: Vec<usize> =
                list.iter().enumerate().filter(|(_, d)| d.name.to_lowercase().contains(&low)).map(|(i, _)| i).collect();
            match hits.len() {
                1 => hits[0],
                0 => return Err(format!("「{entry}」に当たるデバイスが無い")),
                _ => {
                    let names: Vec<String> = hits.iter().map(|&i| format!("#{i} {}", list[i].name)).collect();
                    return Err(format!("「{entry}」に当たるデバイスが複数ある：{}。#番号で選ぶ", names.join(" / ")));
                }
            }
        };
        if out.contains(&idx) {
            return Err(format!("「{entry}」：同じデバイス（#{idx} {}）を2回は選べない", list[idx].name));
        }
        out.push(idx);
    }
    if out.is_empty() {
        return Err(format!("{env} が空"));
    }
    Ok(out)
}

fn device_list_text(list: &[DeviceInfo]) -> String {
    device_list_text_of(list, "再生")
}

fn device_list_text_of(list: &[DeviceInfo], kind: &str) -> String {
    let mut s = String::new();
    for (i, d) in list.iter().enumerate() {
        s.push_str(&format!("  #{i}  {}{}\n      id = {}\n", d.name, if d.is_default { "  [既定]" } else { "" }, d.id));
    }
    if list.is_empty() {
        s.push_str(&format!("  （{kind}デバイスが見つからない）\n"));
    }
    s
}

/// 1970-01-01 からの日数 → (年, 月, 日)（Howard Hinnant の civil_from_days）
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// FILETIME（1601-01-01 からの 100ns）→ `YYYYMMDD-HHMMSS`
fn stamp_from_filetime(ft: u64) -> String {
    const UNIX_EPOCH_AS_FILETIME: u64 = 116_444_736_000_000_000;
    let secs = (ft as i128 - UNIX_EPOCH_AS_FILETIME as i128).div_euclid(10_000_000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// 記録の親フォルダ：`PL_PROBE_OUT`、無ければ `%USERPROFILE%\Documents\pl-player-probe`
fn probe_base_dir(out_env: Option<&str>, userprofile: Option<&str>) -> Result<PathBuf, String> {
    if let Some(o) = out_env.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(o));
    }
    match userprofile.map(str::trim).filter(|s| !s.is_empty()) {
        Some(home) => Ok(PathBuf::from(home).join("Documents").join("pl-player-probe")),
        None => Err("USERPROFILE が無い。PL_PROBE_OUT で記録の置き場所を指定する".into()),
    }
}

/// 中央値（並べ替えるので写しを渡す）
fn median_i64(mut v: Vec<i64>) -> Option<i64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

// ----------------------------------------------------------------------
// コールバックの記録と、その分析（drift_probe の要約）
// ----------------------------------------------------------------------

/// コールバック1回の記録（コールバックの中で作り、輪で記録係へ渡す）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CbRecord {
    dev: u8,
    open_no: u32,
    seq: u64,
    frames: u32,
    written_before: u64,
    callback_ns: u64,
    playback_ns: u64,
    qpc_ns: u64,
    stalled: bool,
}

/// drift の分析に使う点：(x＝書いた総フレーム数, y＝playback − L − 最初の playback（ns）)。
/// 開いて `skip_ns` までと頭打ちの回は外す。
fn drift_points(records: &[CbRecord], skip_ns: u64) -> Vec<(f64, f64)> {
    let Some(first) = records.first() else { return vec![] };
    let l = first.playback_ns.saturating_sub(first.callback_ns) as i64;
    let p0 = first.playback_ns as i64;
    records
        .iter()
        .filter(|r| !r.stalled && r.callback_ns >= first.callback_ns + skip_ns)
        .map(|r| (r.written_before as f64, (r.playback_ns as i64 - l - p0) as f64))
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
struct DriftAnalysis {
    callbacks: usize,
    stalls: usize,
    frames_min: u32,
    frames_max: u32,
    /// 最初の回の playback − callback（＝cpal が開いたときに取った GetStreamLatency）
    l_ns: u64,
    /// 外す区間のあと・頭打ちでない回の playback − callback − L の最小・最大（ns。負なら L が途中で小さくなった疑い）
    buffered_min_ns: i64,
    buffered_max_ns: i64,
    fit: Option<Line>,
    ppm: Option<f64>,
    resid: ResidualStats,
    /// 参考：written_before と callback の当てはめから出した ppm
    ppm_by_callback: Option<f64>,
    /// 開いて skip〜skip+2秒だけで当てた残り
    early: Option<ResidualStats>,
    entry_minus_cb_median_ns: i64,
    entry_minus_cb_max_ns: i64,
    gap_median_ns: u64,
    gap_max_ns: u64,
    /// 間が中央値の3倍を超えた回数
    gaps_over_3x: usize,
    span_s: f64,
}

fn analyze_drift(records: &[CbRecord], rate: u32, skip_ns: u64) -> Option<DriftAnalysis> {
    let first = records.first()?;
    let last = records.last()?;
    let mut a = DriftAnalysis {
        callbacks: records.len(),
        stalls: records.iter().filter(|r| r.stalled).count(),
        frames_min: records.iter().map(|r| r.frames).min().unwrap_or(0),
        frames_max: records.iter().map(|r| r.frames).max().unwrap_or(0),
        l_ns: first.playback_ns.saturating_sub(first.callback_ns),
        span_s: last.callback_ns.saturating_sub(first.callback_ns) as f64 / 1e9,
        ..Default::default()
    };
    let after: Vec<&CbRecord> =
        records.iter().filter(|r| !r.stalled && r.callback_ns >= first.callback_ns + skip_ns).collect();
    let buffered: Vec<i64> =
        after.iter().map(|r| r.playback_ns as i64 - r.callback_ns as i64 - a.l_ns as i64).collect();
    a.buffered_min_ns = buffered.iter().copied().min().unwrap_or(0);
    a.buffered_max_ns = buffered.iter().copied().max().unwrap_or(0);

    let pts = drift_points(records, skip_ns);
    a.fit = fit_line(pts.iter().copied());
    if let Some(line) = a.fit {
        a.ppm = Some(ppm_from_slope(line.slope, rate));
        a.resid = residual_stats(pts.iter().copied(), &line);
    }
    let c0 = first.callback_ns as i64;
    let cb_pts: Vec<(f64, f64)> =
        after.iter().map(|r| (r.written_before as f64, (r.callback_ns as i64 - c0) as f64)).collect();
    a.ppm_by_callback = fit_line(cb_pts.iter().copied()).map(|l| ppm_from_slope(l.slope, rate));

    let early_end = first.callback_ns + skip_ns + 2_000_000_000;
    let early_recs: Vec<CbRecord> = records.iter().filter(|r| r.callback_ns <= early_end).copied().collect();
    let early_pts = drift_points(&early_recs, skip_ns);
    a.early = fit_line(early_pts.iter().copied()).map(|l| residual_stats(early_pts.iter().copied(), &l));

    let entry: Vec<i64> = records.iter().map(|r| r.qpc_ns as i64 - r.callback_ns as i64).collect();
    a.entry_minus_cb_median_ns = median_i64(entry.clone()).unwrap_or(0);
    a.entry_minus_cb_max_ns = entry.iter().copied().max().unwrap_or(0);

    let gaps: Vec<i64> = records.windows(2).map(|w| w[1].qpc_ns as i64 - w[0].qpc_ns as i64).collect();
    let med = median_i64(gaps.clone()).unwrap_or(0).max(0) as u64;
    a.gap_median_ns = med;
    a.gap_max_ns = gaps.iter().copied().max().unwrap_or(0).max(0) as u64;
    a.gaps_over_3x = if med > 0 { gaps.iter().filter(|&&g| g as u64 > med * 3).count() } else { 0 };
    Some(a)
}

/// 当てはめの残りを時刻順に（`seq,t_s,written_before,residual_us,stalled`）。頭打ちの回も、線との差を出す
fn residual_rows(records: &[CbRecord], line: &Line, skip_ns: u64) -> Vec<(u64, f64, u64, f64, bool)> {
    let Some(first) = records.first() else { return vec![] };
    let l = first.playback_ns.saturating_sub(first.callback_ns) as i64;
    let p0 = first.playback_ns as i64;
    records
        .iter()
        .filter(|r| r.callback_ns >= first.callback_ns + skip_ns)
        .map(|r| {
            let y = (r.playback_ns as i64 - l - p0) as f64;
            let resid = y - line.at(r.written_before as f64);
            (
                r.seq,
                r.callback_ns.saturating_sub(first.callback_ns) as f64 / 1e9,
                r.written_before,
                resid / 1000.0,
                r.stalled,
            )
        })
        .collect()
}

// ----------------------------------------------------------------------
// 録った音からクリックの立ち上がりを探す（loopback_click_probe）
// ----------------------------------------------------------------------

/// クリックの前後を弱める長さ（ms）。`click_burst` に渡す値
const CLICK_FADE_MS: f64 = 0.5;

/// 高さを見分ける窓の長さ（ms。ハン窓）。2ms のハン窓は、狙った高さから 1kHz 以上離れた「ずっと鳴っている音」を消す
/// （クリックの 1kHz・3kHz・5kHz どうしと、確かめに使う 2kHz・4kHz・6kHz はどれも 1kHz の整数倍だけ離れている）。
/// 48kHz なら 96 標本ちょうど。44.1kHz は 88 標本で、わずかに漏れる（テストで確かめている）。
/// 決め方（2026-11-16、作った波形で）：ほかのデバイスのクリックが 1.25ms 以内に重なったときの偏りの最大が、
/// 1ms の素の窓では約 0.15ms、2ms のハン窓では約 0.06ms、3ms のハン窓では約 0.055ms。2ms で足りる
const TONE_WINDOW_MS: f64 = 2.0;

/// 見つけたとみなす大きさの下限（振幅。-80 dBFS）
const ONSET_MIN_PEAK: f32 = 1e-4;
/// 窓の中の中央値（雑音の目安）に対して、これだけ大きくないと「小さい」で断る（dB）
const ONSET_MIN_SNR_DB: f64 = 20.0;
/// 同じ高さで、見つけたものの半分以上の山がほかにもあれば「紛らわしい」で断る
const ONSET_MAX_RIVAL: f32 = 0.5;
/// クリックの所で「隣の高さ（＋1kHz）」がふだんより上がった分が、きれいなクリックより山のこの割合以上大きければ「クリックの音ではない」で断る
const ONSET_MAX_OFF_EXTRA: f32 = 0.3;

fn tone_window_len(rate: u32) -> usize {
    ((rate as f64 * TONE_WINDOW_MS / 1000.0).round() as usize).max(4)
}

/// 高さ `freq_hz` の成分の大きさ（振幅）を、`m` 標本のハン窓をずらしながら出す。戻り値の i 番目は標本 i〜i+m−1 の窓（中心は i＋(m−1)/2）。
///
/// ハン窓 h(k) = 0.5 − 0.5·cos(2π(k+0.5)/m) を掛けた和は、3つの高さ（ω と ω±2π/m）の素の和の組み合わせになる。
/// 素の和は累積和で出せるので、窓をずらしても長さに比例する手間で済み、誤差もたまらない（f64）
fn tone_envelope(x: &[f32], rate: u32, freq_hz: f64, m: usize) -> Vec<f32> {
    if m == 0 || x.len() < m {
        return vec![];
    }
    let w = 2.0 * std::f64::consts::PI * freq_hz / rate as f64;
    let big = 2.0 * std::f64::consts::PI / m as f64;
    // pre[j][n] = Σ_{k<n} x[k]·e^{−i·ws[j]·k}（実部・虚部）
    let ws = [w, w - big, w + big];
    let pre: Vec<Vec<(f64, f64)>> = ws
        .iter()
        .map(|&wj| {
            let mut v = Vec::with_capacity(x.len() + 1);
            let (mut re, mut im) = (0f64, 0f64);
            v.push((re, im));
            for (k, &xv) in x.iter().enumerate() {
                let ph = wj * k as f64;
                re += xv as f64 * ph.cos();
                im -= xv as f64 * ph.sin();
                v.push((re, im));
            }
            v
        })
        .collect();
    let norm = m as f64 / 2.0; // Σh
    (0..=x.len() - m)
        .map(|i| {
            let sum = |j: usize| (pre[j][i + m].0 - pre[j][i].0, pre[j][i + m].1 - pre[j][i].1);
            let (a, b, c) = (sum(0), sum(1), sum(2));
            // Σ x·e^{−iωn}·h(n−i) = 0.5·A − 0.25·(e^{−iθ}·B + e^{iθ}·C)、θ = 2π/m·(i − 0.5)
            let th = big * (i as f64 - 0.5);
            let (ct, st) = (th.cos(), th.sin());
            let eb = (ct * b.0 + st * b.1, ct * b.1 - st * b.0);
            let ec = (ct * c.0 - st * c.1, ct * c.1 + st * c.0);
            let re = 0.5 * a.0 - 0.25 * (eb.0 + ec.0);
            let im = 0.5 * a.1 - 0.25 * (eb.1 + ec.1);
            (2.0 * (re * re + im * im).sqrt() / norm) as f32
        })
        .collect()
}

fn median_f32(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut w = v.to_vec();
    w.sort_unstable_by(|a, b| a.total_cmp(b));
    w[w.len() / 2]
}

/// 山 `p` を含み、`half` 以上が続く範囲 [a, b]（大きさの並びの添字）
fn above_run(env: &[f32], p: usize, half: f32) -> (usize, usize) {
    let (mut a, mut b) = (p, p);
    while a > 0 && env[a - 1] >= half {
        a -= 1;
    }
    while b + 1 < env.len() && env[b + 1] >= half {
        b += 1;
    }
    (a, b)
}

/// 半分の大きさを横切る位置（小数。大きさの並びの添字）。左は a−1 → a、右は b → b+1 の間を直線で補う
fn half_crossings(env: &[f32], a: usize, b: usize, half: f32) -> (f64, f64) {
    let xl = {
        let (lo, hi) = (env[a - 1] as f64, env[a] as f64);
        (a - 1) as f64 + if hi > lo { (half as f64 - lo) / (hi - lo) } else { 0.5 }
    };
    let xr = {
        let (hi, lo) = (env[b] as f64, env[b + 1] as f64);
        b as f64 + if hi > lo { (hi - half as f64) / (hi - lo) } else { 0.5 }
    };
    (xl, xr)
}

/// きれいなクリック（雑音の無いもの）の形。立ち上がりの決め方と「形が違う」の検査の物差し
#[derive(Clone, Copy, Debug, PartialEq)]
struct ClickShape {
    /// 半分の大きさを超えている長さ（標本）
    width: f64,
    /// クリックの最初の標本から、半分を超えている範囲の中心まで（標本）
    mid_offset: f64,
    /// 中心での「隣の高さ」の大きさ ÷ 山
    off_ratio: f32,
}

fn click_shape(rate: u32, freq_hz: f64, burst_ms: f64, m: usize) -> Option<ClickShape> {
    let pad = m + 8;
    let burst = click_burst(freq_hz, rate, burst_ms, CLICK_FADE_MS, 1.0);
    let mut x = vec![0f32; pad];
    x.extend_from_slice(&burst);
    x.resize(x.len() + pad, 0.0);
    let env = tone_envelope(&x, rate, freq_hz, m);
    let (p, peak) = env.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1))?;
    let half = peak / 2.0;
    let (a, b) = above_run(&env, p, half);
    if a == 0 || b + 1 >= env.len() {
        return None;
    }
    let (xl, xr) = half_crossings(&env, a, b, half);
    let mid_env = (xl + xr) / 2.0;
    let off = tone_envelope(&x, rate, freq_hz + 1000.0, m).get(mid_env.round() as usize).copied().unwrap_or(0.0);
    Some(ClickShape {
        width: xr - xl,
        mid_offset: mid_env + (m as f64 - 1.0) / 2.0 - pad as f64,
        off_ratio: off / peak,
    })
}

/// 見つけたクリック
#[derive(Clone, Copy, Debug, PartialEq)]
struct Onset {
    /// クリックの最初の標本の位置（渡した並びの中の標本番号。小数）
    pos: f64,
    /// 大きさ（振幅。1.0 がフルスケール）
    peak: f32,
    /// 窓の中のその高さの大きさの中央値（雑音の目安）との比（dB）
    snr_db: f64,
    /// 半分の大きさを超えていた長さ（ms）
    width_ms: f64,
}

/// クリックが取れなかった理由
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum HitMiss {
    /// 狙った時刻の前後が録れていない（流れが来ていない・始まる前・終わった後）
    NotCaptured,
    /// 前後の途中で録音が途切れた（取りこぼし・時刻の飛び）
    Gap,
    /// その高さの音が無い
    NoSignal,
    /// 雑音に対して小さい
    Weak,
    /// 窓の端で切れている
    Truncated,
    /// 長さがクリックと違う
    WrongShape,
    /// ほかの高さも同じくらい鳴っている（クリックの音ではない）
    NotTone,
    /// 同じ高さの山が2つ以上ある
    Ambiguous,
}

impl HitMiss {
    const ALL: [HitMiss; 8] = [
        HitMiss::NotCaptured,
        HitMiss::Gap,
        HitMiss::NoSignal,
        HitMiss::Weak,
        HitMiss::Truncated,
        HitMiss::WrongShape,
        HitMiss::NotTone,
        HitMiss::Ambiguous,
    ];
    fn code(self) -> &'static str {
        match self {
            HitMiss::NotCaptured => "not_captured",
            HitMiss::Gap => "gap",
            HitMiss::NoSignal => "no_signal",
            HitMiss::Weak => "weak",
            HitMiss::Truncated => "truncated",
            HitMiss::WrongShape => "wrong_shape",
            HitMiss::NotTone => "not_tone",
            HitMiss::Ambiguous => "ambiguous",
        }
    }
    fn text(self) -> &'static str {
        match self {
            HitMiss::NotCaptured => "録れていない",
            HitMiss::Gap => "途切れ",
            HitMiss::NoSignal => "音が無い",
            HitMiss::Weak => "小さい",
            HitMiss::Truncated => "窓の端で切れた",
            HitMiss::WrongShape => "長さが違う",
            HitMiss::NotTone => "クリックの音ではない",
            HitMiss::Ambiguous => "紛らわしい",
        }
    }
}

/// 録った音 `x`（モノラル。狙った時刻の前後だけ）から、高さ `freq_hz`・長さ `burst_ms` のクリックの立ち上がりを探す。
///
/// 1. 2ms のハン窓でその高さの大きさを出す（ほかの高さは消える）
/// 2. いちばん大きい山の、半分の大きさを横切る左右の位置を出し、その中心からクリックの最初の標本を逆算する
///    （クリックは前後対称なので、中心は雑音に強い。物差しは `click_shape`）
/// 3. 断る：音が無い・雑音に対して小さい・窓の端で切れている・長さが違う・隣の高さも一緒に上がる（物音）・同じ高さの山がほかにもある
fn find_click_onset(x: &[f32], rate: u32, freq_hz: f64, burst_ms: f64) -> Result<Onset, HitMiss> {
    let m = tone_window_len(rate);
    let shape = click_shape(rate, freq_hz, burst_ms, m).ok_or(HitMiss::NoSignal)?;
    let env = tone_envelope(x, rate, freq_hz, m);
    if env.len() < 3 {
        return Err(HitMiss::NotCaptured);
    }
    let (p, peak) = env.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).ok_or(HitMiss::NoSignal)?;
    if !(peak >= ONSET_MIN_PEAK) {
        return Err(HitMiss::NoSignal);
    }
    let noise = median_f32(&env);
    let snr_db = 20.0 * (peak as f64 / (noise as f64).max(1e-12)).log10();
    if snr_db < ONSET_MIN_SNR_DB {
        return Err(HitMiss::Weak);
    }
    let half = peak / 2.0;
    let (a, b) = above_run(&env, p, half);
    if a == 0 || b + 1 >= env.len() {
        return Err(HitMiss::Truncated);
    }
    let (xl, xr) = half_crossings(&env, a, b, half);
    let width = xr - xl;
    let tol = (0.5e-3 * rate as f64).max(0.25 * shape.width);
    if (width - shape.width).abs() > tol {
        return Err(HitMiss::WrongShape);
    }
    let mid_env = (xl + xr) / 2.0;
    // 隣の高さ（＋1kHz）が、クリックの所で窓の中のふだん（中央値）よりどれだけ上がったか。
    // 上がり方で見るので、ずっと鳴っているほかの音（曲など）では断らず、一瞬だけ全部の高さが鳴る音（物音・プチッという音）だけを断る
    let off_env = tone_envelope(x, rate, freq_hz + 1000.0, m);
    let off_rise = off_env.get(mid_env.round() as usize).copied().unwrap_or(0.0) - median_f32(&off_env);
    if off_rise / peak > shape.off_ratio + ONSET_MAX_OFF_EXTRA {
        return Err(HitMiss::NotTone);
    }
    let burst_len = (burst_ms / 1000.0 * rate as f64).round() as usize;
    let guard = m + burst_len;
    let (lo, hi) = (a.saturating_sub(guard), (b + guard).min(env.len() - 1));
    let rival = env.iter().enumerate().filter(|(i, _)| *i < lo || *i > hi).map(|(_, v)| *v).fold(0f32, f32::max);
    if rival >= ONSET_MAX_RIVAL * peak {
        return Err(HitMiss::Ambiguous);
    }
    Ok(Onset {
        pos: mid_env + (m as f64 - 1.0) / 2.0 - shape.mid_offset,
        peak,
        snr_db,
        width_ms: width * 1000.0 / rate as f64,
    })
}

/// 録った流れの1パケット（コールバック1回）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CapPacket {
    /// この流れのフレームの通し番号。取りこぼしたパケットのぶんも進める（つながりの検査に使う）
    pos: u64,
    /// このパケットより前に、実際に手元へ渡したフレームの数（取りこぼしたぶんは数えない）
    stored_before: u64,
    frames: u32,
    /// cpal の入力の `capture`（WASAPI の GetBuffer が返す QPC。パケットの最初のフレームの時刻）
    capture_ns: u64,
    /// cpal の入力の `callback`
    callback_ns: u64,
    /// コールバックに入ったときに自分で読んだ QPC
    qpc_ns: u64,
}

/// パケットの時刻のつながりを許す幅。これより飛んだら、そこで途切れたとみなす
const CAPTURE_GAP_TOL_NS: u64 = 1_000_000;

fn packet_end_ns(p: &CapPacket, rate: u32) -> f64 {
    p.capture_ns as f64 + p.frames as f64 * 1e9 / rate as f64
}

/// 次のパケットの時刻 − 前のパケットから見込んだ時刻（ns）
fn packet_jump_ns(prev: &CapPacket, next: &CapPacket, rate: u32) -> f64 {
    next.capture_ns as f64 - packet_end_ns(prev, rate)
}

/// 次のパケットが前のパケットにつながっているか（取りこぼしが無く、時刻の飛びが幅の中）
fn packets_connect(prev: &CapPacket, next: &CapPacket, rate: u32, tol_ns: u64) -> bool {
    next.pos == prev.pos + prev.frames as u64 && packet_jump_ns(prev, next, rate).abs() <= tol_ns as f64
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum WindowMiss {
    /// まだ録れていない（あとで来る）
    NotYet,
    NotCaptured,
    Gap,
}

/// 時刻 from〜to を覆うフレームの範囲。戻り値は（最初の通し番号, 数, 最初のフレームの stored_before での番号）。packets は時刻順
fn window_positions(packets: &[CapPacket], rate: u32, from_ns: u64, to_ns: u64, tol_ns: u64) -> Result<(u64, usize, u64), WindowMiss> {
    let last = packets.last().ok_or(WindowMiss::NotYet)?;
    if packet_end_ns(last, rate) < to_ns as f64 {
        return Err(WindowMiss::NotYet);
    }
    let i0 = packets.iter().position(|p| packet_end_ns(p, rate) > from_ns as f64).ok_or(WindowMiss::NotYet)?;
    let first = &packets[i0];
    if first.capture_ns > from_ns + tol_ns {
        return Err(WindowMiss::NotCaptured);
    }
    let mut i = i0;
    while packet_end_ns(&packets[i], rate) < to_ns as f64 {
        let next = packets.get(i + 1).ok_or(WindowMiss::NotYet)?;
        if !packets_connect(&packets[i], next, rate, tol_ns) {
            return Err(WindowMiss::Gap);
        }
        i += 1;
    }
    // 時刻の差 → フレーム数は整数で（切り捨て・切り上げが浮動小数の丸めでずれないように）
    let frames_floor = |dt_ns: u64| (dt_ns as u128 * rate as u128 / 1_000_000_000) as u64;
    let frames_ceil = |dt_ns: u64| (dt_ns as u128 * rate as u128).div_ceil(1_000_000_000) as u64;
    let skip = frames_floor(from_ns.saturating_sub(first.capture_ns));
    let start = first.pos + skip.min(first.frames as u64);
    let lastp = &packets[i];
    let upto = frames_ceil(to_ns.saturating_sub(lastp.capture_ns)).min(lastp.frames as u64);
    let end = lastp.pos + upto;
    Ok((start, end.saturating_sub(start) as usize, first.stored_before + (start - first.pos)))
}

/// フレームの通し番号（小数）→ QPC の ns。そのフレームを含むパケットの時刻から数える
fn pos_to_ns(packets: &[CapPacket], pos: f64, rate: u32) -> Option<f64> {
    let p = packets.iter().rev().find(|p| p.pos as f64 <= pos)?;
    if pos >= (p.pos + p.frames as u64) as f64 {
        return None;
    }
    Some(p.capture_ns as f64 + (pos - p.pos as f64) * 1e9 / rate as f64)
}

/// 録った流れの、手元に残している所（狙った時刻の前後を切り出すため）。古い所は捨てて、長く流しても増えない
struct CaptureTrack {
    rate: u32,
    /// モノラルにした標本（stored_before の番号 `trimmed` から）
    samples: VecDeque<f32>,
    trimmed: u64,
    packets: VecDeque<CapPacket>,
    last: Option<CapPacket>,
    // 要約用
    n_packets: u64,
    breaks: u64,
    jump_n: u64,
    jump_sq_sum: f64,
    jump_max_abs: f64,
}

/// パケットが前とつながったか
#[derive(Clone, Copy, Debug, PartialEq)]
enum PacketLink {
    First,
    Connected(f64),
    Broken(f64),
}

impl CaptureTrack {
    fn new(rate: u32) -> Self {
        Self {
            rate,
            samples: VecDeque::new(),
            trimmed: 0,
            packets: VecDeque::new(),
            last: None,
            n_packets: 0,
            breaks: 0,
            jump_n: 0,
            jump_sq_sum: 0.0,
            jump_max_abs: 0.0,
        }
    }
    fn add_packet(&mut self, p: CapPacket) -> PacketLink {
        let link = match self.last {
            None => PacketLink::First,
            Some(prev) => {
                let j = packet_jump_ns(&prev, &p, self.rate);
                if packets_connect(&prev, &p, self.rate, CAPTURE_GAP_TOL_NS) {
                    self.jump_n += 1;
                    self.jump_sq_sum += j * j;
                    self.jump_max_abs = self.jump_max_abs.max(j.abs());
                    PacketLink::Connected(j)
                } else {
                    self.breaks += 1;
                    PacketLink::Broken(j)
                }
            }
        };
        self.last = Some(p);
        self.n_packets += 1;
        self.packets.push_back(p);
        link
    }
    fn add_samples(&mut self, s: &[f32]) {
        self.samples.extend(s.iter().copied());
    }
    /// 時刻 from〜to の標本と、最初の標本の通し番号
    fn window(&mut self, from_ns: u64, to_ns: u64) -> Result<(Vec<f32>, u64), WindowMiss> {
        let (start, n, stored) = window_positions(self.packets.make_contiguous(), self.rate, from_ns, to_ns, CAPTURE_GAP_TOL_NS)?;
        let i0 = stored.checked_sub(self.trimmed).ok_or(WindowMiss::NotCaptured)? as usize;
        if i0 + n > self.samples.len() {
            return Err(WindowMiss::NotYet);
        }
        Ok((self.samples.range(i0..i0 + n).copied().collect(), start))
    }
    fn pos_to_ns(&mut self, pos: f64) -> Option<f64> {
        pos_to_ns(self.packets.make_contiguous(), pos, self.rate)
    }
    /// `keep_from_ns` より前に終わるパケットと、その標本を捨てる（最後のパケットは残す）
    fn trim_before(&mut self, keep_from_ns: u64) {
        while self.packets.len() > 1 && packet_end_ns(&self.packets[0], self.rate) < keep_from_ns as f64 {
            self.packets.pop_front();
        }
        if let Some(front) = self.packets.front() {
            let drop_n = (front.stored_before.saturating_sub(self.trimmed) as usize).min(self.samples.len());
            self.samples.drain(..drop_n);
            self.trimmed += drop_n as u64;
        }
    }
    fn jump_rms_ns(&self) -> f64 {
        if self.jump_n == 0 {
            0.0
        } else {
            (self.jump_sq_sum / self.jump_n as f64).sqrt()
        }
    }
}

/// コールバックの中で、録った音をモノラルにして輪へ渡す役。輪に空きが無ければ、そのパケットを丸ごと捨てて数える
struct CaptureFeed {
    samples: rtrb::Producer<f32>,
    packets: rtrb::Producer<CapPacket>,
    pos: u64,
    stored: u64,
    dropped: Arc<AtomicU64>,
}

impl CaptureFeed {
    fn push(&mut self, frames: usize, mono: impl Iterator<Item = f32>, capture_ns: u64, callback_ns: u64, qpc_ns: u64) {
        if frames == 0 {
            return;
        }
        if self.samples.slots() >= frames && self.packets.slots() >= 1 {
            let rec = CapPacket { pos: self.pos, stored_before: self.stored, frames: frames as u32, capture_ns, callback_ns, qpc_ns };
            for v in mono.take(frames) {
                let _ = self.samples.push(v);
            }
            // 標本を先に書き、パケットの記録はあと（取り出す側はパケット → 標本の順に読むので、読んだパケットの標本は必ずそろう）
            let _ = self.packets.push(rec);
            self.stored += frames as u64;
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        self.pos += frames as u64;
    }
}

/// 差の並びの平均・揺れ（標準偏差）・最小・最大
#[derive(Clone, Copy, Debug, PartialEq)]
struct DiffStats {
    n: usize,
    mean: f64,
    std: f64,
    min: f64,
    max: f64,
}

fn diff_stats(v: &[f64]) -> Option<DiffStats> {
    if v.is_empty() {
        return None;
    }
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let var = v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    Some(DiffStats {
        n: v.len(),
        mean,
        std: var.sqrt(),
        min: v.iter().copied().fold(f64::INFINITY, f64::min),
        max: v.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    })
}

// ======================================================================
// ここから下は実際のデバイスを使う部分
// ======================================================================

#[cfg(windows)]
fn qpc_now_ns() -> u64 {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    static FREQ: OnceLock<i64> = OnceLock::new();
    let freq = *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: 書き込み先は手元の i64
        unsafe {
            let _ = QueryPerformanceFrequency(&mut f);
        }
        f
    });
    let mut c = 0i64;
    // SAFETY: 書き込み先は手元の i64
    unsafe {
        let _ = QueryPerformanceCounter(&mut c);
    }
    qpc_to_ns(c, freq)
}

#[cfg(not(windows))]
fn qpc_now_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64
}

#[cfg(windows)]
fn qpc_freq() -> i64 {
    use windows::Win32::System::Performance::QueryPerformanceFrequency;
    let mut f = 0i64;
    // SAFETY: 書き込み先は手元の i64
    unsafe {
        let _ = QueryPerformanceFrequency(&mut f);
    }
    f
}

#[cfg(not(windows))]
fn qpc_freq() -> i64 {
    0
}

/// 今のローカル時刻の `YYYYMMDD-HHMMSS`
#[cfg(windows)]
fn local_stamp() -> String {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    let unix_100ns =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() / 100).unwrap_or(0);
    let ft = unix_100ns as u64 + 116_444_736_000_000_000;
    let utc = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
    let mut local = FILETIME::default();
    // SAFETY: どちらも手元の FILETIME
    let ok = unsafe { FileTimeToLocalFileTime(&utc, &mut local) }.is_ok();
    if ok {
        stamp_from_filetime(((local.dwHighDateTime as u64) << 32) | local.dwLowDateTime as u64)
    } else {
        format!("{}Z", stamp_from_filetime(ft))
    }
}

#[cfg(not(windows))]
fn local_stamp() -> String {
    let unix_100ns =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() / 100).unwrap_or(0);
    format!("{}Z", stamp_from_filetime(unix_100ns as u64 + 116_444_736_000_000_000))
}

fn instant_ns(i: cpal::StreamInstant) -> u64 {
    i.as_nanos().min(u64::MAX as u128) as u64
}

fn env_str(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// 数の環境変数。数でない・NaN・無限・範囲の外は、デバイスを開く前に止める
fn env_f64_in(name: &str, default: f64, min: f64, max: f64) -> f64 {
    let v = match env_str(name) {
        None => default,
        Some(s) => s.parse().unwrap_or_else(|_| panic!("{name}=「{s}」が数ではない")),
    };
    check_range(name, v, min, max).unwrap_or_else(|e| panic!("{e}"))
}

fn env_bool(name: &str, default: bool) -> bool {
    match env_str(name).as_deref() {
        None => default,
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        Some(s) => panic!("{name}=「{s}」は 1 か 0 で"),
    }
}

/// 選んだデバイス
struct Picked {
    dev_no: u8,
    info: DeviceInfo,
}

/// `PL_PROBE_DEVICES` からデバイスを選ぶ。無い・合わないときは一覧を出して失敗する（音は出さない）。
/// `extra` は一覧のあとに足す文（loopback_click_probe の録音デバイスの一覧）
fn pick_devices(min: usize, max: usize, probe: &str, extra: &str) -> Vec<Picked> {
    let list = devices::list_output_devices();
    let usage = format!(
        "\n再生デバイスの一覧：\n{}\n指定の仕方：PL_PROBE_DEVICES に、鳴らすデバイスを | で区切って並べる（{min}〜{max} 個。先頭が基準）。\n\
         1つずつ default（Windows の既定）／#番号／ID／名前／名前の一部 のどれか。\n\
         例（PowerShell）：$env:PL_PROBE_DEVICES = \"#0|#2\"\n\
         例（Git Bash）：PL_PROBE_DEVICES='#0|#2' cargo test --features test-support --test sync_probe -- --ignored --exact {probe} --nocapture\n{extra}",
        device_list_text(&list)
    );
    let Some(spec) = env_str("PL_PROBE_DEVICES") else {
        panic!("PL_PROBE_DEVICES が無い。{usage}");
    };
    let idx = select_devices(&list, &spec).unwrap_or_else(|e| panic!("{e}{usage}"));
    if idx.len() < min || idx.len() > max {
        panic!("{probe} はデバイスを {min}〜{max} 個（指定は {} 個）。{usage}", idx.len());
    }
    idx.into_iter()
        .enumerate()
        .map(|(n, i)| Picked { dev_no: n as u8 + 1, info: list[i].clone() })
        .collect()
}

/// 録音デバイス（入力）の一覧。名前を読むだけで、開かない
fn list_input_devices() -> Vec<DeviceInfo> {
    let host = devices::host();
    let default_id = host.default_input_device().as_ref().and_then(devices::device_id);
    let mut out = vec![];
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            let Some(id) = devices::device_id(&d) else { continue };
            out.push(DeviceInfo { is_default: Some(&id) == default_id.as_ref(), name: devices::device_name(&d), kind: devices::device_kind(&d), id });
        }
    }
    out
}

/// ID で録音デバイスを探す
fn find_input(id: &str) -> Option<cpal::Device> {
    let host = devices::host();
    if let Ok(parsed) = id.parse::<cpal::DeviceId>() {
        if let Some(d) = host.device_by_id(&parsed) {
            return Some(d);
        }
    }
    host.input_devices().ok()?.find(|d| devices::device_id(d).as_deref() == Some(id))
}

/// 記録のフォルダを作る。リポジトリの中は断る（要約にデバイスの名前と ID が入り、公開物に混ざりうるため）。
/// 同じ秒に作ったフォルダがあれば、後ろに -2・-3… を付けて別にする
fn make_run_dir() -> PathBuf {
    let base = probe_base_dir(env_str("PL_PROBE_OUT").as_deref(), std::env::var("USERPROFILE").ok().as_deref())
        .unwrap_or_else(|e| panic!("{e}"));
    let base = std::path::absolute(&base).unwrap_or_else(|e| panic!("記録の置き場所を絶対パスにできない：{e}"));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest.parent().unwrap_or(manifest);
    if path_is_inside(&base, repo) {
        panic!("PL_PROBE_OUT がリポジトリの中を指している。記録にはデバイスの名前と ID が入るので、リポジトリの外に置く");
    }
    fs::create_dir_all(&base).unwrap_or_else(|e| panic!("記録のフォルダを作れない（{}）：{e}", base.display()));
    let stamp = local_stamp();
    for n in 1..1000 {
        let dir = if n == 1 { base.join(&stamp) } else { base.join(format!("{stamp}-{n}")) };
        match fs::create_dir(&dir) {
            Ok(()) => return dir,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("記録のフォルダを作れない（{}）：{e}", dir.display()),
        }
    }
    panic!("記録のフォルダの名前が決まらない（{}）", base.display());
}

#[derive(Clone, Copy, Debug)]
struct StreamFormat {
    rate: u32,
    channels: usize,
    format: SampleFormat,
}

/// 今のアプリ（`output.rs`）と同じ：既定の形式・既定のバッファ
fn default_format(device: &cpal::Device) -> Result<StreamFormat, String> {
    let s = device.default_output_config().map_err(|e| format!("default_output_config: {e}"))?;
    Ok(StreamFormat { rate: s.sample_rate(), channels: s.channels() as usize, format: s.sample_format() })
}

/// コールバックごとに書き手へ渡す時刻
#[derive(Clone, Copy, Debug)]
struct CbTiming {
    written_before: u64,
    frames: usize,
    channels: usize,
    callback_ns: u64,
    playback_ns: u64,
    qpc_ns: u64,
    stalled: bool,
    first_callback_ns: u64,
    first_playback_ns: u64,
    l_ns: u64,
}

/// コールバックの中で音を書く役。`out` は 0 で埋めてから渡す（f32 のインターリーブ）。重い処理をしない
trait Render: Send + 'static {
    fn render(&mut self, out: &mut [f32], t: &CbTiming);
}

struct Silence;
impl Render for Silence {
    fn render(&mut self, _: &mut [f32], _: &CbTiming) {}
}

/// ストリームごとの、コールバックの外へ知らせる数
#[derive(Default)]
struct Health {
    dropped_records: AtomicU64,
    underruns: AtomicU64,
    errors: Mutex<Vec<String>>,
}

struct Opened {
    /// 止めるときは take() して落とす
    stream: Option<cpal::Stream>,
    fmt: StreamFormat,
    buffer_size: Option<u32>,
    /// 自分の QPC − stream.now()（ns）。13.1 の 2「1ms 未満か」
    now_diff_ns: i64,
    open_start_ns: u64,
    open_done_ns: u64,
}

fn open_stream(
    device: &cpal::Device,
    fmt: StreamFormat,
    dev_no: u8,
    open_no: u32,
    renderer: Box<dyn Render>,
    log: rtrb::Producer<CbRecord>,
    health: Arc<Health>,
) -> Result<Opened, String> {
    let open_start_ns = qpc_now_ns();
    let config =
        cpal::StreamConfig { channels: fmt.channels as u16, sample_rate: fmt.rate, buffer_size: cpal::BufferSize::Default };
    let ch = fmt.channels;
    let stream = match fmt.format {
        SampleFormat::F32 => build::<f32>(device, &config, ch, dev_no, open_no, renderer, log, health),
        SampleFormat::I16 => build::<i16>(device, &config, ch, dev_no, open_no, renderer, log, health),
        SampleFormat::I32 => build::<i32>(device, &config, ch, dev_no, open_no, renderer, log, health),
        SampleFormat::U16 => build::<u16>(device, &config, ch, dev_no, open_no, renderer, log, health),
        SampleFormat::F64 => build::<f64>(device, &config, ch, dev_no, open_no, renderer, log, health),
        other => Err(format!("扱っていない形式：{other:?}")),
    }?;
    stream.play().map_err(|e| format!("play: {e}"))?;
    let open_done_ns = qpc_now_ns();
    let q1 = qpc_now_ns();
    let now = instant_ns(stream.now());
    let q2 = qpc_now_ns();
    let now_diff_ns = ((q1 / 2 + q2 / 2) as i64) - now as i64;
    let buffer_size = stream.buffer_size().ok();
    Ok(Opened { stream: Some(stream), fmt, buffer_size, now_diff_ns, open_start_ns, open_done_ns })
}

#[allow(clippy::too_many_arguments)]
fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    dev_no: u8,
    open_no: u32,
    mut renderer: Box<dyn Render>,
    mut log: rtrb::Producer<CbRecord>,
    health: Arc<Health>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let ch = channels.max(1);
    let mut scratch: Vec<f32> = Vec::with_capacity(1 << 18);
    let mut seq = 0u64;
    let mut written = 0u64;
    let mut prev_pb: Option<u64> = None;
    let (mut first_cb, mut first_pb, mut l_ns) = (0u64, 0u64, 0u64);
    let h = health.clone();
    let data_cb = move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
        let qpc = qpc_now_ns();
        let ts = info.timestamp();
        let cb = instant_ns(ts.callback);
        let pb = instant_ns(ts.playback);
        let frames = data.len() / ch;
        if seq == 0 {
            first_cb = cb;
            first_pb = pb;
            l_ns = pb.saturating_sub(cb);
        }
        let stalled = is_stalled(prev_pb, pb);
        prev_pb = Some(pb);
        let rec = CbRecord {
            dev: dev_no,
            open_no,
            seq,
            frames: frames as u32,
            written_before: written,
            callback_ns: cb,
            playback_ns: pb,
            qpc_ns: qpc,
            stalled,
        };
        if log.push(rec).is_err() {
            h.dropped_records.fetch_add(1, Ordering::Relaxed);
        }
        let timing = CbTiming {
            written_before: written,
            frames,
            channels: ch,
            callback_ns: cb,
            playback_ns: pb,
            qpc_ns: qpc,
            stalled,
            first_callback_ns: first_cb,
            first_playback_ns: first_pb,
            l_ns,
        };
        // 容量の中なら増やさない（大きすぎるバッファのときだけ増える）
        scratch.clear();
        scratch.resize(data.len(), 0.0);
        renderer.render(&mut scratch, &timing);
        for (d, s) in data.iter_mut().zip(scratch.iter()) {
            *d = T::from_sample(s.clamp(-1.0, 1.0));
        }
        seq += 1;
        written += frames as u64;
    };
    let h2 = health;
    let err_cb = move |e: cpal::Error| {
        if let Ok(mut v) = h2.errors.lock() {
            if v.len() < 100 {
                v.push(format!("{:?}: {e}", e.kind()));
            }
        }
    };
    device.build_output_stream::<T, _, _>(*config, data_cb, err_cb, None).map_err(|e| format!("build_output_stream: {e}"))
}

/// コールバックの記録を受け取り、CSV に書き、分析用に持つ
struct CbSink {
    csv: BufWriter<File>,
    logs: Vec<rtrb::Consumer<CbRecord>>,
    records: BTreeMap<(u8, u32), Vec<CbRecord>>,
}

const LOG_CAPACITY: usize = 1 << 16;

impl CbSink {
    fn new(dir: &Path) -> Self {
        let mut csv = BufWriter::new(File::create(dir.join("callbacks.csv")).expect("callbacks.csv"));
        writeln!(
            csv,
            "dev,open,seq,frames,written_before,callback_ns,playback_ns,qpc_entry_ns,stalled,pb_minus_cb_us,entry_minus_cb_us"
        )
        .unwrap();
        Self { csv, logs: vec![], records: BTreeMap::new() }
    }
    fn new_log(&mut self) -> rtrb::Producer<CbRecord> {
        let (p, c) = rtrb::RingBuffer::new(LOG_CAPACITY);
        self.logs.push(c);
        p
    }
    fn drain(&mut self) {
        for c in self.logs.iter_mut() {
            while let Ok(r) = c.pop() {
                let _ = writeln!(
                    self.csv,
                    "{},{},{},{},{},{},{},{},{},{:.1},{:.1}",
                    r.dev,
                    r.open_no,
                    r.seq,
                    r.frames,
                    r.written_before,
                    r.callback_ns,
                    r.playback_ns,
                    r.qpc_ns,
                    r.stalled as u8,
                    (r.playback_ns as i64 - r.callback_ns as i64) as f64 / 1000.0,
                    (r.qpc_ns as i64 - r.callback_ns as i64) as f64 / 1000.0,
                );
                self.records.entry((r.dev, r.open_no)).or_default().push(r);
            }
        }
        // 止まって写したものが空になったものは捨てる
        self.logs.retain(|c| !(c.is_abandoned() && c.slots() == 0));
    }
    fn flush(&mut self) {
        let _ = self.csv.flush();
    }
    fn records(&self, dev: u8, open_no: u32) -> &[CbRecord] {
        self.records.get(&(dev, open_no)).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// QPC の時刻まで待つ（その間、記録を書き出す）
fn wait_until(target_ns: u64, mut tick: impl FnMut()) {
    loop {
        let now = qpc_now_ns();
        if now >= target_ns {
            return;
        }
        let left = Duration::from_nanos(target_ns - now);
        std::thread::sleep(left.min(Duration::from_millis(50)));
        tick();
    }
}

fn errors_text(h: &Health) -> String {
    let v = h.errors.lock().map(|v| v.clone()).unwrap_or_default();
    if v.is_empty() {
        "なし".into()
    } else {
        v.join(" / ")
    }
}

fn ms(ns: f64) -> String {
    format!("{:.3} ms", ns / 1e6)
}

struct DevRun {
    picked: Picked,
    opened: Opened,
    health: Arc<Health>,
}

fn device_header(d: &DevRun) -> String {
    let o = &d.opened;
    format!(
        "== デバイス {}{}：{}\n  id：{}\n  形式：{} Hz・{} ch・{:?}・バッファ {}\n  自前の QPC − stream.now()：{}（1ms 未満なら、cpal の時刻と自前の QPC を同じ時計として比べてよい）{}\n  開くのにかかった時間：{}\n",
        d.picked.dev_no,
        if d.picked.dev_no == 1 { "（基準）" } else { "" },
        d.picked.info.name,
        d.picked.info.id,
        o.fmt.rate,
        o.fmt.channels,
        o.fmt.format,
        o.buffer_size.map(|b| format!("{b} フレーム")).unwrap_or_else(|| "不明".into()),
        ms(o.now_diff_ns as f64),
        if o.now_diff_ns.abs() >= 1_000_000 { " ← 1ms 以上ずれている" } else { "" },
        ms(o.open_done_ns.saturating_sub(o.open_start_ns) as f64),
    )
}

fn drift_summary_text(runs: &[DevRun], sink: &CbSink, skip_ns: u64, started: &str, planned_s: f64, elapsed_s: f64, done: bool) -> String {
    let mut s = String::new();
    s.push_str("同時再生の計測：drift_probe（時計の速さ・位置の揺れ・L・頭打ち）\n");
    s.push_str(&format!(
        "開始：{started}（ローカル時刻）\n流した長さ：{elapsed_s:.1} 秒（指定 {planned_s:.0} 秒）{}\n",
        if done { "" } else { "　※途中の要約（まだ流している）" }
    ));
    s.push_str(&format!("当てはめから外した最初の区間：{} ms／QPC の周波数：{} Hz\n\n", skip_ns / 1_000_000, qpc_freq()));
    s.push_str("読み方：時計の速さは「書いた総フレーム数」と「playback − L」の直線の傾きから（QPC に対して。＋は名目のレートより速い）。\n");
    s.push_str("L は cpal が開いたときに1回だけ取る GetStreamLatency（最初のコールバックでは playback − callback = L）。\n\n");
    let mut ppms: Vec<(u8, Option<f64>)> = vec![];
    for d in runs {
        s.push_str(&device_header(d));
        let recs = sink.records(d.picked.dev_no, 1);
        match analyze_drift(recs, d.opened.fmt.rate, skip_ns) {
            None => s.push_str("  コールバックが1回も来ていない\n"),
            Some(a) => {
                s.push_str(&format!(
                    "  コールバック：{} 回（1回 {}〜{} フレーム）・間の中央値 {}・最大 {}・中央値の3倍を超えて空いた回数 {}\n",
                    a.callbacks,
                    a.frames_min,
                    a.frames_max,
                    ms(a.gap_median_ns as f64),
                    ms(a.gap_max_ns as f64),
                    a.gaps_over_3x
                ));
                s.push_str(&format!("  L（最初の playback − callback）：{}\n", ms(a.l_ns as f64)));
                s.push_str(&format!(
                    "  playback − callback − L：最小 {}・最大 {}（書いて未消化の量。最小が負なら L が途中で小さくなった疑い）→ {}\n",
                    ms(a.buffered_min_ns as f64),
                    ms(a.buffered_max_ns as f64),
                    if a.buffered_min_ns < -50_000 { "L が変わった疑いあり" } else { "L は変わっていないと見てよい" }
                ));
                match (a.ppm, a.fit) {
                    (Some(p), Some(line)) => s.push_str(&format!(
                        "  時計の速さ：{p:+.3} ppm（実のレート {:.4} Hz）\n",
                        1e9 / line.slope
                    )),
                    _ => s.push_str("  時計の速さ：点が足りず出せない\n"),
                }
                if let Some(p) = a.ppm_by_callback {
                    s.push_str(&format!("  参考：callback の時刻で当てた速さ {p:+.3} ppm（コールバックの揺れを含むので粗い）\n"));
                }
                s.push_str(&format!(
                    "  当てはめの残り：標準偏差 {}・最大 {}（{} 点。のこぎり形は drift_residuals_dev{}.csv）\n",
                    ms(a.resid.std),
                    ms(a.resid.max_abs),
                    a.resid.n,
                    d.picked.dev_no
                ));
                if let Some(e) = a.early {
                    s.push_str(&format!(
                        "  開いて2秒までだけで当てた残り：標準偏差 {}・最大 {}（{} 点。基準の選び方の目安）\n",
                        ms(e.std),
                        ms(e.max_abs),
                        e.n
                    ));
                }
                s.push_str(&format!(
                    "  頭打ち：{} 回（{:.3}%）\n",
                    a.stalls,
                    if a.callbacks > 0 { a.stalls as f64 * 100.0 / a.callbacks as f64 } else { 0.0 }
                ));
                s.push_str(&format!(
                    "  自前の QPC − callback：中央値 {}・最大 {}\n",
                    ms(a.entry_minus_cb_median_ns as f64),
                    ms(a.entry_minus_cb_max_ns as f64)
                ));
                ppms.push((d.picked.dev_no, a.ppm));
            }
        }
        s.push_str(&format!(
            "  記録の取りこぼし：{} 件・エラー：{}\n\n",
            d.health.dropped_records.load(Ordering::Relaxed),
            errors_text(&d.health)
        ));
    }
    if runs.len() >= 2 {
        s.push_str("== 基準（デバイス1）との差\n");
        let base = ppms.iter().find(|(n, _)| *n == 1).and_then(|(_, p)| *p);
        for (n, p) in ppms.iter().filter(|(n, _)| *n != 1) {
            match (base, p) {
                (Some(b), Some(p)) => {
                    let diff = p - b;
                    s.push_str(&format!(
                        "  デバイス {n}：{diff:+.3} ppm → 1時間で {:+.2} ms（＋は基準より速く進む＝同じ曲を流すと先へ行く）{}\n",
                        drift_ms_per_hour(diff),
                        if drift_ms_per_hour(diff).abs() > 10.0 { "　← 10ms を超える" } else { "" }
                    ));
                }
                _ => s.push_str(&format!("  デバイス {n}：出せない\n")),
            }
        }
    }
    s
}

#[allow(clippy::too_many_arguments)]
fn write_drift_outputs(dir: &Path, runs: &[DevRun], sink: &CbSink, skip_ns: u64, started: &str, planned_s: f64, elapsed_s: f64, done: bool) {
    let text = drift_summary_text(runs, sink, skip_ns, started, planned_s, elapsed_s, done);
    let _ = fs::write(dir.join("drift_summary.txt"), &text);
    for d in runs {
        let recs = sink.records(d.picked.dev_no, 1);
        let Some(line) = fit_line(drift_points(recs, skip_ns).into_iter()) else { continue };
        let path = dir.join(format!("drift_residuals_dev{}.csv", d.picked.dev_no));
        if let Ok(f) = File::create(&path) {
            let mut w = BufWriter::new(f);
            let _ = writeln!(w, "seq,t_s,written_before,residual_us,stalled");
            for (seq, t, wb, r, st) in residual_rows(recs, &line, skip_ns) {
                let _ = writeln!(w, "{seq},{t:.6},{wb},{r:.2},{}", st as u8);
            }
            let _ = w.flush();
        }
    }
    if done {
        println!("{text}");
    }
}

/// 13.1 の 1・2・6：選んだデバイスへ無音を流し続け、コールバックごとに時刻を記録する
#[test]
#[ignore]
fn drift_probe() {
    let picked = pick_devices(1, 16, "drift_probe", "");
    let secs = env_f64_in("PL_PROBE_SECS", 3600.0, 1.0, MAX_PROBE_SECS);
    let skip_ns = (env_f64_in("PL_PROBE_SKIP_MS", 200.0, 0.0, 10_000.0) * 1e6) as u64;
    let summary_every = env_f64_in("PL_PROBE_SUMMARY_EVERY_SEC", 600.0, 10.0, 86_400.0);
    let dir = make_run_dir();
    let started = local_stamp();
    let _ = qpc_now_ns();
    println!("記録の置き場所：{}", dir.display());
    let mut sink = CbSink::new(&dir);
    let mut runs = vec![];
    for p in picked {
        let device = devices::find_output(Some(&p.info.id)).unwrap_or_else(|| panic!("開けない：{}", p.info.name));
        let fmt = default_format(&device).unwrap_or_else(|e| panic!("{}：{e}", p.info.name));
        let health = Arc::new(Health::default());
        let log = sink.new_log();
        let opened = open_stream(&device, fmt, p.dev_no, 1, Box::new(Silence), log, health.clone())
            .unwrap_or_else(|e| panic!("{}：{e}", p.info.name));
        println!("デバイス {}：{}（{} Hz・{} ch・{:?}）", p.dev_no, p.info.name, fmt.rate, fmt.channels, fmt.format);
        runs.push(DevRun { picked: p, opened, health });
    }
    let t0 = qpc_now_ns();
    let end = t0 + (secs * 1e9) as u64;
    let mut next_flush = t0 + 1_000_000_000;
    let mut next_progress = t0 + 60_000_000_000;
    // 最初の要約は1分（指定がもっと短ければその間隔）で書く。早めに止めても要約が残るように
    let mut next_summary = t0 + (summary_every.min(60.0) * 1e9) as u64;
    while qpc_now_ns() < end {
        std::thread::sleep(Duration::from_millis(100));
        sink.drain();
        let now = qpc_now_ns();
        if now >= next_flush {
            sink.flush();
            next_flush = now + 1_000_000_000;
        }
        if now >= next_progress {
            let counts: Vec<String> =
                runs.iter().map(|d| format!("{}:{}", d.picked.dev_no, sink.records(d.picked.dev_no, 1).len())).collect();
            println!("{:.0} / {secs:.0} 秒（コールバック {}）", (now - t0) as f64 / 1e9, counts.join(" "));
            next_progress = now + 60_000_000_000;
        }
        if now >= next_summary {
            write_drift_outputs(&dir, &runs, &sink, skip_ns, &started, secs, (now - t0) as f64 / 1e9, false);
            next_summary = now + (summary_every * 1e9) as u64;
        }
    }
    let elapsed = (qpc_now_ns() - t0) as f64 / 1e9;
    for d in runs.iter_mut() {
        drop(d.opened.stream.take());
    }
    sink.drain();
    sink.flush();
    write_drift_outputs(&dir, &runs, &sink, skip_ns, &started, secs, elapsed, true);
    println!("記録：{}", dir.display());
}

// ----------------------------------------------------------------------
// click_probe
// ----------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum ClickStatus {
    Ok,
    Late,
    TooSoon,
}

#[derive(Clone, Copy, Debug)]
struct ClickEvent {
    dev: u8,
    open_no: u32,
    click: u32,
    status: ClickStatus,
    target_ns: u64,
    start_frame: i64,
    aimed_ns: i64,
    fit_minus_raw_ns: f64,
    fit_points: u32,
    l_ns: u64,
    used_fit: bool,
    decided_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AimMode {
    Fit,
    Raw,
}

/// 決めたクリック（どの回を、どのフレームから）
#[derive(Clone, Copy, Debug)]
struct Scheduled {
    click: usize,
    start_frame: i64,
}

struct ClickRenderer {
    dev: u8,
    open_no: u32,
    rate: u32,
    burst: Vec<f32>,
    targets: Arc<Vec<u64>>,
    /// 途中参加：この回だけを狙う
    only: Option<usize>,
    next: usize,
    sched: Option<Scheduled>,
    win: FitWindow,
    aim: AimMode,
    subtract_l: bool,
    skip_ns: u64,
    /// 途中参加のときだけ：開いて skip のあと、これより前の回は狙わない
    gate_ns: Option<u64>,
    /// 狙うフレームが、書く範囲の先この数以内に来たら決める
    horizon_frames: i64,
    events: rtrb::Producer<ClickEvent>,
    health: Arc<Health>,
}

/// クリックの決め方（純粋。テストで確かめる）。戻り値：決めた／遅れた／まだ
#[derive(Debug, PartialEq)]
enum Decide {
    Now { start_frame: i64 },
    Late { start_frame: i64 },
    NotYet,
}

fn decide_click(line: &Line, target_rel_ns: f64, written_before: u64, frames: usize, horizon_frames: i64) -> Decide {
    let k = aim_frame(line, target_rel_ns);
    if k < written_before as i64 {
        Decide::Late { start_frame: k }
    } else if k < written_before as i64 + frames as i64 + horizon_frames {
        Decide::Now { start_frame: k }
    } else {
        Decide::NotYet
    }
}

impl ClickRenderer {
    fn push_event(&mut self, ev: ClickEvent) {
        if self.events.push(ev).is_err() {
            self.health.dropped_records.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Render for ClickRenderer {
    fn render(&mut self, out: &mut [f32], t: &CbTiming) {
        // y は「playback − 最初の playback」。L を引くなら狙いを L だけ後ろへずらす（L は1本のストリームの中で変わらない）
        let y = t.playback_ns as i64 - t.first_playback_ns as i64;
        if !t.stalled && t.callback_ns >= t.first_callback_ns + self.skip_ns {
            self.win.push(t.written_before as f64, y as f64, t.callback_ns);
        }
        let origin = t.first_playback_ns as i64 - if self.subtract_l { t.l_ns as i64 } else { 0 };
        let slope_nominal = 1e9 / self.rate as f64;
        while self.sched.is_none() && self.next < self.targets.len() {
            if let Some(o) = self.only {
                if self.next < o {
                    self.next = o;
                    continue;
                }
                if self.next > o {
                    break;
                }
            }
            let target = self.targets[self.next];
            if let Some(g) = self.gate_ns {
                if target < t.first_callback_ns + self.skip_ns + g {
                    let ev = ClickEvent {
                        dev: self.dev,
                        open_no: self.open_no,
                        click: self.next as u32,
                        status: ClickStatus::TooSoon,
                        target_ns: target,
                        start_frame: -1,
                        aimed_ns: 0,
                        fit_minus_raw_ns: 0.0,
                        fit_points: self.win.len() as u32,
                        l_ns: t.l_ns,
                        used_fit: false,
                        decided_ns: t.qpc_ns,
                    };
                    self.push_event(ev);
                    self.next += 1;
                    continue;
                }
            }
            let (lx, ly) = self.win.latest().unwrap_or((t.written_before as f64, y as f64));
            let raw = Line::through(lx, ly, slope_nominal);
            let fit = if self.win.len() >= 4 { self.win.fit() } else { None };
            let (line, used_fit) = match (self.aim, fit) {
                (AimMode::Fit, Some(f)) => (f, true),
                _ => (raw, false),
            };
            let target_rel = (target as i64 - origin) as f64;
            let fit_minus_raw = fit.map(|f| (aim_frame(&f, target_rel) - aim_frame(&raw, target_rel)) as f64 * slope_nominal).unwrap_or(0.0);
            match decide_click(&line, target_rel, t.written_before, t.frames, self.horizon_frames) {
                Decide::NotYet => break,
                d @ (Decide::Now { .. } | Decide::Late { .. }) => {
                    let (k, status) = match d {
                        Decide::Now { start_frame } => (start_frame, ClickStatus::Ok),
                        Decide::Late { start_frame } => (start_frame, ClickStatus::Late),
                        Decide::NotYet => unreachable!(),
                    };
                    let ev = ClickEvent {
                        dev: self.dev,
                        open_no: self.open_no,
                        click: self.next as u32,
                        status,
                        target_ns: target,
                        start_frame: k,
                        aimed_ns: line.at(k as f64).round() as i64 + origin,
                        fit_minus_raw_ns: fit_minus_raw,
                        fit_points: self.win.len() as u32,
                        l_ns: t.l_ns,
                        used_fit,
                        decided_ns: t.qpc_ns,
                    };
                    self.push_event(ev);
                    if status == ClickStatus::Ok {
                        self.sched = Some(Scheduled { click: self.next, start_frame: k });
                    } else {
                        self.next += 1;
                    }
                }
            }
        }
        if let Some(s) = self.sched {
            let ch = t.channels;
            for j in 0..t.frames {
                if let Some(i) = burst_index(t.written_before + j as u64, s.start_frame, self.burst.len()) {
                    out[j * ch..(j + 1) * ch].fill(self.burst[i]);
                }
            }
            if (t.written_before + t.frames as u64) as i64 >= s.start_frame + self.burst.len() as i64 {
                self.sched = None;
                self.next = s.click + 1;
            }
        }
    }
}

struct ClickSink {
    csv: BufWriter<File>,
    logs: Vec<rtrb::Consumer<ClickEvent>>,
    events: Vec<ClickEvent>,
    first_target: u64,
    subtract_l: bool,
}

impl ClickSink {
    fn new(dir: &Path, first_target: u64, subtract_l: bool) -> Self {
        let mut csv = BufWriter::new(File::create(dir.join("click_targets.csv")).expect("click_targets.csv"));
        writeln!(
            csv,
            "dev,open,click,status,target_ns,target_rel_ms,start_frame,aimed_ns,aim_err_us,fit_minus_raw_us,fit_points,l_us,subtract_l,aim,decided_ns"
        )
        .unwrap();
        Self { csv, logs: vec![], events: vec![], first_target, subtract_l }
    }
    fn new_log(&mut self) -> rtrb::Producer<ClickEvent> {
        let (p, c) = rtrb::RingBuffer::new(4096);
        self.logs.push(c);
        p
    }
    fn drain(&mut self) {
        for c in self.logs.iter_mut() {
            while let Ok(e) = c.pop() {
                let status = match e.status {
                    ClickStatus::Ok => "ok",
                    ClickStatus::Late => "late",
                    ClickStatus::TooSoon => "too_soon",
                };
                let aim_err = if e.status == ClickStatus::TooSoon { 0.0 } else { (e.aimed_ns - e.target_ns as i64) as f64 / 1000.0 };
                let _ = writeln!(
                    self.csv,
                    "{},{},{},{},{},{:.3},{},{},{:.2},{:.2},{},{:.1},{},{},{}",
                    e.dev,
                    e.open_no,
                    e.click,
                    status,
                    e.target_ns,
                    (e.target_ns as i64 - self.first_target as i64) as f64 / 1e6,
                    e.start_frame,
                    e.aimed_ns,
                    aim_err,
                    e.fit_minus_raw_ns / 1000.0,
                    e.fit_points,
                    e.l_ns as f64 / 1000.0,
                    self.subtract_l as u8,
                    if e.used_fit { "fit" } else { "raw" },
                    e.decided_ns,
                );
                self.events.push(e);
            }
        }
        self.logs.retain(|c| !(c.is_abandoned() && c.slots() == 0));
        let _ = self.csv.flush();
    }
}

struct ClickParams {
    burst_ms: f64,
    amp: f32,
    aim: AimMode,
    subtract_l: bool,
    skip_ns: u64,
    window_ns: u64,
}

#[allow(clippy::too_many_arguments)]
fn open_click_stream(
    p: &Picked,
    open_no: u32,
    targets: &Arc<Vec<u64>>,
    only: Option<usize>,
    gate_ns: Option<u64>,
    cp: &ClickParams,
    sink: &mut CbSink,
    csink: &mut ClickSink,
) -> Result<(Opened, Arc<Health>), String> {
    let device = devices::find_output(Some(&p.info.id)).ok_or_else(|| format!("開けない：{}", p.info.name))?;
    let fmt = default_format(&device)?;
    let health = Arc::new(Health::default());
    let r = ClickRenderer {
        dev: p.dev_no,
        open_no,
        rate: fmt.rate,
        burst: click_burst(click_freq_for(p.dev_no), fmt.rate, cp.burst_ms, CLICK_FADE_MS, cp.amp),
        targets: targets.clone(),
        only,
        next: 0,
        sched: None,
        win: FitWindow::new(4096, cp.window_ns),
        aim: cp.aim,
        subtract_l: cp.subtract_l,
        skip_ns: cp.skip_ns,
        gate_ns,
        horizon_frames: (fmt.rate / 20) as i64,
        events: csink.new_log(),
        health: health.clone(),
    };
    let log = sink.new_log();
    let opened = open_stream(&device, fmt, p.dev_no, open_no, Box::new(r), log, health.clone())?;
    Ok((opened, health))
}

/// 13.1 の 3・5：2つ以上のデバイスに、同じ QPC 時刻を狙ってクリックを鳴らす（録音は作者が別に）
#[test]
#[ignore]
fn click_probe() {
    run_click_probe(ClickMode::Record);
}

/// 13.1 の 3・5 を PC の中で：click_probe と同じ狙い方で鳴らし、鳴らしている出力をループバックで（と、指定した録音デバイスも）録って、
/// クリックの立ち上がりの QPC 時刻から、ずれを自動で出す。機器の中・USB の先の遅れは測れない（ファイルの頭）
#[test]
#[ignore]
fn loopback_click_probe() {
    run_click_probe(ClickMode::Loopback);
}

/// click_probe（録音は作者が別に）か、loopback_click_probe（PC の中で録る）か
#[derive(Clone, Copy, Debug, PartialEq)]
enum ClickMode {
    Record,
    Loopback,
}

/// クリックの大きさの既定（dBFS）。ループバックは耳で聞かないので小さめ
const CLICK_DB_RECORD: f64 = -12.0;
const CLICK_DB_LOOPBACK: f64 = -30.0;

impl ClickMode {
    fn probe(self) -> &'static str {
        match self {
            ClickMode::Record => "click_probe",
            ClickMode::Loopback => "loopback_click_probe",
        }
    }
    fn default_db(self) -> f64 {
        match self {
            ClickMode::Record => CLICK_DB_RECORD,
            ClickMode::Loopback => CLICK_DB_LOOPBACK,
        }
    }
    fn summary_file(self) -> &'static str {
        match self {
            ClickMode::Record => "click_summary.txt",
            ClickMode::Loopback => "loopback_summary.txt",
        }
    }
}

fn run_click_probe(mode: ClickMode) {
    let probe = mode.probe();
    let loopback = mode == ClickMode::Loopback;
    // 録音デバイスの一覧（ループバックのときだけ）。名前を読むだけで、まだ何も開かない
    let inputs = if loopback { list_input_devices() } else { vec![] };
    let input_usage = if loopback {
        format!(
            "\n録音デバイスの一覧（PL_PROBE_CAPTURE で、鳴らしている出力のループバックに加えて録るものを | で区切って選ぶ。付けなければループバックだけ）：\n{}",
            device_list_text_of(&inputs, "録音")
        )
    } else {
        String::new()
    };
    let picked = if loopback { pick_devices(1, 3, probe, &input_usage) } else { pick_devices(2, 3, probe, "") };
    let interval_ms = env_f64_in("PL_PROBE_CLICK_INTERVAL_MS", 2000.0, 50.0, 3_600_000.0);
    let count = env_f64_in("PL_PROBE_CLICK_COUNT", 10.0, 1.0, 100_000.0) as usize;
    let total = env_str("PL_PROBE_SECS").map(|s| s.parse::<f64>().unwrap_or_else(|_| panic!("PL_PROBE_SECS=「{s}」が数ではない")));
    let starts = click_burst_starts(env_str("PL_PROBE_CLICK_AT_SEC").as_deref(), total).unwrap_or_else(|e| panic!("{e}"));
    let join = env_bool("PL_PROBE_JOIN", false);
    let open_before_ms = env_f64_in("PL_PROBE_JOIN_OPEN_BEFORE_MS", 600.0, 100.0, 10_000.0);
    let gate_ms = env_f64_in("PL_PROBE_GATE_MS", 80.0, 0.0, 2_000.0);
    let lead_ms = env_f64_in("PL_PROBE_CLICK_LEAD_MS", 3000.0, 500.0, 60_000.0);
    let aim = match env_str("PL_PROBE_AIM").as_deref() {
        None | Some("fit") => AimMode::Fit,
        Some("raw") => AimMode::Raw,
        Some(s) => panic!("PL_PROBE_AIM=「{s}」は fit か raw"),
    };
    let cp = ClickParams {
        burst_ms: env_f64_in("PL_PROBE_CLICK_MS", 5.0, 1.0, 100.0),
        amp: level_amp("PL_PROBE_CLICK_DB", env_f64_in("PL_PROBE_CLICK_DB", mode.default_db(), f64::MIN, f64::MAX)).unwrap_or_else(|e| panic!("{e}")),
        aim,
        subtract_l: env_bool("PL_PROBE_SUBTRACT_L", true),
        skip_ns: (env_f64_in("PL_PROBE_SKIP_MS", 200.0, 0.0, 10_000.0) * 1e6) as u64,
        window_ns: (env_f64_in("PL_PROBE_FIT_WINDOW_MS", 3000.0, 100.0, 60_000.0) * 1e6) as u64,
    };
    if interval_ms < cp.burst_ms + 50.0 {
        panic!("PL_PROBE_CLICK_INTERVAL_MS はクリックの長さ＋50ms 以上に");
    }
    if join && interval_ms < open_before_ms + 400.0 {
        panic!("途中参加では PL_PROBE_CLICK_INTERVAL_MS を PL_PROBE_JOIN_OPEN_BEFORE_MS＋400ms 以上に");
    }
    if join && picked.len() < 2 {
        panic!("途中参加（PL_PROBE_JOIN=1）はデバイスを2つ以上（2台目を開き直す）");
    }
    // ループバック：録音デバイスの指定と、狙った時刻の前後どこまでを見るか。どちらもデバイスを開く前に確かめる
    let capture_picked: Vec<Picked> = match (loopback, env_str("PL_PROBE_CAPTURE")) {
        (false, _) | (true, None) => vec![],
        (true, Some(spec)) => {
            let idx = select_from(&inputs, &spec, "録音", "PL_PROBE_CAPTURE").unwrap_or_else(|e| panic!("{e}{input_usage}"));
            if idx.len() > MAX_CAPTURE_DEVICES {
                panic!("PL_PROBE_CAPTURE は {MAX_CAPTURE_DEVICES} 個まで（指定は {} 個）{input_usage}", idx.len());
            }
            idx.into_iter().enumerate().map(|(n, i)| Picked { dev_no: n as u8 + 1, info: inputs[i].clone() }).collect()
        }
    };
    let look_ms = if loopback { env_f64_in("PL_PROBE_CAPTURE_WINDOW_MS", 200.0, 20.0, 500.0) } else { 0.0 };
    if loopback && interval_ms < 2.0 * look_ms + cp.burst_ms + 10.0 {
        panic!("PL_PROBE_CLICK_INTERVAL_MS は PL_PROBE_CAPTURE_WINDOW_MS の2倍＋クリックの長さ＋10ms 以上に（前後の回が窓に入らないように）");
    }
    let dir = make_run_dir();
    let started = local_stamp();
    println!("記録の置き場所：{}", dir.display());
    let base = qpc_now_ns() + (lead_ms * 1e6) as u64 + if join { (open_before_ms * 1e6) as u64 } else { 0 };
    let targets = Arc::new(click_targets(base, &starts, count, interval_ms));
    let mut sink = CbSink::new(&dir);
    let mut csink = ClickSink::new(&dir, targets[0], cp.subtract_l);

    let mut runs: Vec<DevRun> = vec![];
    let mut join_dev: Option<Picked> = None;
    for p in picked {
        if join && p.dev_no == 2 {
            join_dev = Some(p);
            continue;
        }
        let (opened, health) =
            open_click_stream(&p, 1, &targets, None, None, &cp, &mut sink, &mut csink).unwrap_or_else(|e| panic!("{}：{e}", p.info.name));
        println!("デバイス {}：{}（{} Hz・{} ch・{:?}・{} Hz のクリック）", p.dev_no, p.info.name, opened.fmt.rate, opened.fmt.channels, opened.fmt.format, click_freq_for(p.dev_no));
        runs.push(DevRun { picked: p, opened, health });
    }
    // ループバック：鳴らす各デバイス（途中参加の2台目も、開いたまま録る）と、指定した録音デバイスを録り始める
    let mut capt: Option<Capturing> = if loopback {
        let mut c = Capturing::new(&dir, targets.clone(), (look_ms * 1e6) as u64, cp.burst_ms);
        let mut all: Vec<&Picked> = runs.iter().map(|d| &d.picked).collect();
        if let Some(p2) = join_dev.as_ref() {
            all.push(p2);
        }
        all.sort_by_key(|p| p.dev_no);
        let dev_nos: Vec<u8> = all.iter().map(|p| p.dev_no).collect();
        for p in all {
            c.open(CapSource::Loopback(p.dev_no), &p.info, vec![p.dev_no]).unwrap_or_else(|e| panic!("{} のループバック：{e}", p.info.name));
        }
        for p in &capture_picked {
            c.open(CapSource::Input(p.dev_no), &p.info, dev_nos.clone()).unwrap_or_else(|e| panic!("録音デバイス {}：{e}", p.info.name));
        }
        for s in &c.streams {
            println!("{}：{}（{} Hz・{} ch・{:?}）", s.label, s.title, s.fmt.rate, s.fmt.channels, s.fmt.format);
        }
        Some(c)
    } else {
        None
    };
    let last = *targets.last().unwrap();
    println!(
        "クリック {} 回。最初は {:.1} 秒後、最後は {:.1} 秒後。{}",
        targets.len(),
        (targets[0] as f64 - qpc_now_ns() as f64) / 1e9,
        (last as f64 - qpc_now_ns() as f64) / 1e9,
        if join { "2台目は毎回開き直す（途中参加）" } else { "" }
    );

    // 要約の頭（設定）。始める前に一度書いておく（途中で止めても、設定とデバイスは残る）
    let mut head = String::new();
    if loopback {
        head.push_str("同時再生の計測：loopback_click_probe（同じ QPC 時刻を狙ったクリックを、PC の中で録ってずれを出す）\n");
    } else {
        head.push_str("同時再生の計測：click_probe（同じ QPC 時刻を狙ったクリック）\n");
    }
    head.push_str(&format!("開始：{started}（ローカル時刻）\n"));
    head.push_str(&format!(
        "クリック：{} 回（{} 回ずつ・{interval_ms:.0} ms おき・鳴らし始め {:?} 秒）・長さ {:.1} ms・大きさ {:.1} dBFS\n",
        targets.len(),
        count,
        starts,
        cp.burst_ms,
        20.0 * (cp.amp as f64).log10()
    ));
    head.push_str(&format!(
        "狙い方：{}・L を{}・当てはめの窓 {} ms・開いて {} ms は当てはめに使わない{}\n\n",
        if cp.aim == AimMode::Fit { "直近の直線の当てはめ（fit）" } else { "その回の playback 1点（raw）" },
        if cp.subtract_l { "引く" } else { "引かない" },
        cp.window_ns / 1_000_000,
        cp.skip_ns / 1_000_000,
        if join { format!("・途中参加（2台目をクリックの {open_before_ms:.0} ms 前に開き直し、開いて {} ms＋{gate_ms:.0} ms より前の回は狙わない）", cp.skip_ns / 1_000_000) } else { String::new() }
    ));
    if loopback {
        head.push_str(&format!("録り方：狙った時刻の前後 {look_ms:.0} ms の中で、デバイスごとの高さ（1台目 1kHz・2台目 3kHz・3台目 5kHz）を探す\n"));
        head.push_str(LOOPBACK_SCOPE_TEXT);
        head.push('\n');
    }
    {
        let mut e = head.clone();
        for d in &runs {
            e.push_str(&device_header(d));
            e.push('\n');
        }
        if let Some(c) = capt.as_ref() {
            for s in &c.streams {
                e.push_str(&s.header());
            }
            e.push('\n');
        }
        e.push_str("※途中の要約（流している途中か、途中で止めた）。終わると全体の要約で書き直す。\n");
        e.push_str("　狙った時刻とその結果は click_targets.csv（随時書き出し）、コールバックの記録は callbacks.csv（1秒ごと）に残っている。\n");
        if loopback {
            e.push_str("　録った流れのパケットは capture_packets.csv、見つけたクリックは loopback_clicks.csv（どちらも1秒ごと）に残っている。\n");
        }
        let _ = fs::write(dir.join(mode.summary_file()), &e);
    }

    // 途中参加の開き直しの記録
    let mut join_runs: Vec<(u32, usize, DevRun)> = vec![];
    if let Some(p2) = join_dev.as_ref() {
        let mut current: Option<cpal::Stream> = None;
        for (i, &t) in targets.iter().enumerate() {
            wait_until(t.saturating_sub((open_before_ms * 1e6) as u64), || {
                sink.drain();
                csink.drain();
                if let Some(c) = capt.as_mut() {
                    c.tick(&csink.events, false);
                }
            });
            drop(current.take());
            let open_no = i as u32 + 1;
            match open_click_stream(p2, open_no, &targets, Some(i), Some((gate_ms * 1e6) as u64), &cp, &mut sink, &mut csink) {
                Ok((mut opened, health)) => {
                    current = opened.stream.take();
                    join_runs.push((open_no, i, DevRun { picked: Picked { dev_no: p2.dev_no, info: p2.info.clone() }, opened, health }));
                }
                Err(e) => println!("{} 回目の開き直しに失敗：{e}", i + 1),
            }
            sink.drain();
            csink.drain();
            sink.flush();
        }
        wait_until(last + 1_000_000_000, || {
            sink.drain();
            csink.drain();
            if let Some(c) = capt.as_mut() {
                c.tick(&csink.events, false);
            }
        });
        drop(current.take());
    } else {
        let mut next_flush = qpc_now_ns();
        wait_until(last + 1_000_000_000, || {
            sink.drain();
            csink.drain();
            if let Some(c) = capt.as_mut() {
                c.tick(&csink.events, false);
            }
            if qpc_now_ns() >= next_flush {
                sink.flush();
                next_flush = qpc_now_ns() + 1_000_000_000;
            }
        });
    }
    for d in runs.iter_mut() {
        drop(d.opened.stream.take());
    }
    sink.drain();
    csink.drain();
    sink.flush();
    // 録るのは鳴らし終えてから止める（最後の回の窓は last＋窓。上で last＋1秒まで待っている）
    if let Some(c) = capt.as_mut() {
        c.stop();
        c.tick(&csink.events, true);
    }

    // 要約
    let mut s = head.clone();
    for d in &runs {
        s.push_str(&device_header(d));
        let recs = sink.records(d.picked.dev_no, 1);
        if let Some(a) = analyze_drift(recs, d.opened.fmt.rate, cp.skip_ns) {
            s.push_str(&format!("  L：{}・頭打ち {} 回・時計の速さ {}\n", ms(a.l_ns as f64), a.stalls, a.ppm.map(|p| format!("{p:+.3} ppm")).unwrap_or("-".into())));
        }
        click_status_line(&mut s, &csink.events, d.picked.dev_no);
        s.push_str(&format!("  記録の取りこぼし：{} 件・エラー：{}\n\n", d.health.dropped_records.load(Ordering::Relaxed), errors_text(&d.health)));
    }
    if let Some(p2) = join_dev.as_ref() {
        s.push_str(&format!("== デバイス 2（途中参加）：{}\n  id：{}\n", p2.info.name, p2.info.id));
        s.push_str("  回・開くのにかかった時間・自前の QPC − stream.now()・L・開き始めてから最初のコールバックまで・開き終えてから狙ったクリックまで\n");
        for (open_no, i, d) in &join_runs {
            let recs = sink.records(d.picked.dev_no, *open_no);
            let first = recs.first();
            s.push_str(&format!(
                "  {:>3}・{}・{}・{}・{}・{}\n",
                i + 1,
                ms(d.opened.open_done_ns.saturating_sub(d.opened.open_start_ns) as f64),
                ms(d.opened.now_diff_ns as f64),
                first.map(|r| ms(r.playback_ns.saturating_sub(r.callback_ns) as f64)).unwrap_or("-".into()),
                first.map(|r| ms(r.qpc_ns.saturating_sub(d.opened.open_start_ns) as f64)).unwrap_or("-".into()),
                ms(targets[*i].saturating_sub(d.opened.open_done_ns) as f64),
            ));
        }
        click_status_line(&mut s, &csink.events, p2.dev_no);
        s.push('\n');
    }
    if let Some(c) = capt.as_mut() {
        s.push_str(&c.report(&csink.events, join));
        s.push_str("見方：まず「基準との差」の平均と揺れ（10ms 以内か・開始・30分・60分で変わらないか）。取れなかった回は loopback_clicks.csv の status。\n");
        s.push_str("aimed_minus_target_us（選んだフレームの丸め）を引けば、狙い方そのもののずれになる（ふつうは無視できる）。\n");
        s.push_str("録ったパケットの時刻は capture_packets.csv（jump_us がパケットどうしの時刻の飛び）。\n");
    } else {
        s.push_str("測り方：録音（Audacity など）で、同じ回の 1kHz と 3kHz（と 5kHz）の立ち上がりの差を読む。それが実際のずれ。\n");
        s.push_str("click_targets.csv の aim_err_us（選んだフレームの丸め）の差を引けば、狙い方そのもののずれになる（ふつうは無視できる）。\n");
        s.push_str("開始・30分・60分で差が変わらないか。L がデバイスで違うなら PL_PROBE_SUBTRACT_L=1 と 0 の両方で録る。\n");
    }
    let _ = fs::write(dir.join(mode.summary_file()), &s);
    println!("{s}");
    println!("記録：{}", dir.display());
}

fn click_status_line(s: &mut String, events: &[ClickEvent], dev: u8) {
    let mine: Vec<&ClickEvent> = events.iter().filter(|e| e.dev == dev).collect();
    let n = |st: ClickStatus| mine.iter().filter(|e| e.status == st).count();
    s.push_str(&format!(
        "  クリック：鳴らした {} 回・遅れて鳴らさなかった {} 回・開いてすぐで狙わなかった {} 回\n",
        n(ClickStatus::Ok),
        n(ClickStatus::Late),
        n(ClickStatus::TooSoon)
    ));
}

// ----------------------------------------------------------------------
// loopback_click_probe：PC の中で録る（ループバック・録音デバイス）
// ----------------------------------------------------------------------

/// 一緒に録る録音デバイスの上限
const MAX_CAPTURE_DEVICES: usize = 4;

/// 要約に書く「何を測っていて、何を測っていないか」（ファイルの頭の説明と同じ中身）
const LOOPBACK_SCOPE_TEXT: &str = "何を測っていて、何を測っていないか：\n\
  - ループバック（lb1〜lb3）の時刻は、Windows の音のエンジンがその出力デバイスへ渡した音に付けた QPC 時刻（WASAPI の GetBuffer が返す、パケットの最初のフレームの時刻）。\n\
    USB・HDMI・Bluetooth の先と、機器の中の遅れは入らない（耳に届く時刻ではない）\n\
  - 録音デバイス（cap1〜）の時刻は、その入力に入ってきた時刻。仮想ケーブルなら、入口（再生デバイス側）から出口（録音デバイス側）へ渡る遅れと、入力の遅れも入る（OBS が受け取る側に近い）\n\
  - 狙いとの差＝録った立ち上がり − 狙った QPC 時刻。基準との差＝同じ回の1台目（ループバックは lb1 の 1kHz、録音デバイスはその中の 1kHz）との差。lb1 との差＝録音デバイスの中のクリック − lb1 の 1kHz。どれも＋は遅い\n\
  - 立ち上がりは、クリックの最初の標本の時刻（2ms のハン窓でその高さの大きさを出し、半分の大きさを横切る左右の中点から、クリックの形で逆算）。\n\
    作った波形での確かめ：雑音だけなら約 10µs 以内、ほかのデバイスのクリックが 1.25ms 以内に重なると最大約 0.06ms 偏る\n";

/// 録る流れの出どころ。数字はループバックならデバイス番号（PL_PROBE_DEVICES の順）、録音デバイスなら PL_PROBE_CAPTURE の順
#[derive(Clone, Copy, Debug, PartialEq)]
enum CapSource {
    Loopback(u8),
    Input(u8),
}

impl CapSource {
    fn label(self) -> String {
        match self {
            CapSource::Loopback(n) => format!("lb{n}"),
            CapSource::Input(n) => format!("cap{n}"),
        }
    }
}

/// 1つの流れ・1つの高さ・1回のクリックの結果。Ok は（立ち上がりの QPC 時刻 ns, 見つけたもの）
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hit {
    stream: usize,
    dev: u8,
    click: usize,
    res: Result<(f64, Onset), HitMiss>,
}

/// loopback_clicks.csv の1行（要約の材料）
#[derive(Clone, Copy, Debug, PartialEq)]
struct ClickRow {
    stream: usize,
    dev: u8,
    open_no: u32,
    click: usize,
    target_ns: u64,
    /// そのデバイスでその回を鳴らした（click_targets.csv の status が ok）
    played: bool,
    res: Result<(f64, Onset), HitMiss>,
    aimed_minus_target_ns: Option<f64>,
    minus_base_ns: Option<f64>,
    minus_lb1_ns: Option<f64>,
}

impl ClickRow {
    /// 鳴らして、見つかったときの立ち上がり
    fn onset(&self) -> Option<f64> {
        if self.played {
            self.res.ok().map(|(ns, _)| ns)
        } else {
            None
        }
    }
    fn minus_target_ns(&self) -> Option<f64> {
        self.onset().map(|ns| ns - self.target_ns as f64)
    }
    fn status(&self) -> &'static str {
        match (self.played, self.res) {
            (false, _) => "not_played",
            (true, Ok(_)) => "ok",
            (true, Err(m)) => m.code(),
        }
    }
}

/// 結果を行にする：鳴らしたか・基準との差・lb1 との差を足す（純粋。テストで確かめる）
fn click_rows(hits: &[Hit], sources: &[CapSource], targets: &[u64], events: &[ClickEvent]) -> Vec<ClickRow> {
    let by_key: BTreeMap<(usize, u8, usize), &Hit> = hits.iter().map(|h| ((h.stream, h.dev, h.click), h)).collect();
    let ok_event = |dev: u8, click: usize| events.iter().find(|e| e.dev == dev && e.click as usize == click && e.status == ClickStatus::Ok);
    let any_event = |dev: u8, click: usize| events.iter().find(|e| e.dev == dev && e.click as usize == click);
    // 1台目を鳴らして、見つかったときの立ち上がり
    let base_onset = |stream: Option<usize>, click: usize| -> Option<f64> {
        let h = by_key.get(&(stream?, 1, click))?;
        ok_event(1, click)?;
        h.res.ok().map(|(ns, _)| ns)
    };
    let lb1 = sources.iter().position(|s| *s == CapSource::Loopback(1));
    let mut rows: Vec<ClickRow> = by_key
        .values()
        .map(|h| {
            let ev = ok_event(h.dev, h.click);
            let target_ns = targets.get(h.click).copied().unwrap_or(0);
            let mut row = ClickRow {
                stream: h.stream,
                dev: h.dev,
                open_no: ev.or_else(|| any_event(h.dev, h.click)).map(|e| e.open_no).unwrap_or(1),
                click: h.click,
                target_ns,
                played: ev.is_some(),
                res: h.res,
                aimed_minus_target_ns: ev.map(|e| (e.aimed_ns - e.target_ns as i64) as f64),
                minus_base_ns: None,
                minus_lb1_ns: None,
            };
            if let Some(on) = row.onset() {
                let source = sources.get(h.stream).copied();
                if h.dev != 1 {
                    let base_stream = match source {
                        Some(CapSource::Loopback(_)) => lb1,
                        _ => Some(h.stream),
                    };
                    row.minus_base_ns = base_onset(base_stream, h.click).map(|b| on - b);
                }
                if matches!(source, Some(CapSource::Input(_))) {
                    row.minus_lb1_ns = base_onset(lb1, h.click).map(|b| on - b);
                }
            }
            row
        })
        .collect();
    rows.sort_by_key(|r| (r.stream, r.dev, r.click));
    rows
}

/// 1つの流れ・1つの高さの集計
#[derive(Clone, Debug, Default, PartialEq)]
struct SectionStats {
    vs_target: Vec<f64>,
    vs_base: Vec<f64>,
    vs_lb1: Vec<f64>,
    misses: BTreeMap<HitMiss, usize>,
    not_played: usize,
    /// 鳴らしていないのに見つかった（見分けの誤りの目安。0 のはず）
    found_unplayed: usize,
}

fn section_stats<'a>(rows: impl Iterator<Item = &'a ClickRow>) -> SectionStats {
    let mut st = SectionStats::default();
    for r in rows {
        if !r.played {
            st.not_played += 1;
            if r.res.is_ok() {
                st.found_unplayed += 1;
            }
            continue;
        }
        match r.res {
            Ok(_) => {
                st.vs_target.extend(r.minus_target_ns());
                st.vs_base.extend(r.minus_base_ns);
                st.vs_lb1.extend(r.minus_lb1_ns);
            }
            Err(m) => *st.misses.entry(m).or_default() += 1,
        }
    }
    st
}

fn stats_line(name: &str, v: &[f64]) -> String {
    match diff_stats(v) {
        None => format!("  {name}：-\n"),
        Some(d) => format!(
            "  {name}：{} 回・平均 {:+.3} ms・揺れ（標準偏差）{:.3} ms・最小 {:+.3} ms・最大 {:+.3} ms\n",
            d.n,
            d.mean / 1e6,
            d.std / 1e6,
            d.min / 1e6,
            d.max / 1e6
        ),
    }
}

fn opt_ms(v: Option<f64>) -> String {
    v.map(|x| format!("{:+.3}", x / 1e6)).unwrap_or_else(|| "-".into())
}

fn opt_us(v: Option<f64>) -> String {
    v.map(|x| format!("{:.1}", x / 1e3)).unwrap_or_default()
}

/// 録っている流れ1つ（コールバックの外の側）
struct CapStream {
    source: CapSource,
    label: String,
    title: String,
    id: String,
    fmt: StreamFormat,
    stream: Option<cpal::Stream>,
    health: Arc<Health>,
    dropped: Arc<AtomicU64>,
    samples: rtrb::Consumer<f32>,
    packets: rtrb::Consumer<CapPacket>,
    track: CaptureTrack,
    next_click: usize,
    /// 探す高さ（デバイス番号）
    devs: Vec<u8>,
    open_ns: u64,
    now_diff_ns: i64,
}

impl CapStream {
    fn header(&self) -> String {
        let freqs: Vec<String> = self.devs.iter().map(|&d| format!("{} Hz（デバイス {d}）", click_freq_for(d))).collect();
        format!(
            "== {}：{}\n  id：{}\n  形式：{} Hz・{} ch・{:?}・探す高さ {}\n  開くのにかかった時間：{}・自前の QPC − stream.now()：{}\n  \
             パケット：{} 個・途切れ {} 回・時刻の飛び（つながった所）RMS {:.1} µs・最大 {:.1} µs・取りこぼし {} 個・エラー：{}\n",
            self.label,
            self.title,
            self.id,
            self.fmt.rate,
            self.fmt.channels,
            self.fmt.format,
            freqs.join("・"),
            ms(self.open_ns as f64),
            ms(self.now_diff_ns as f64),
            self.track.n_packets,
            self.track.breaks,
            self.track.jump_rms_ns() / 1e3,
            self.track.jump_max_abs / 1e3,
            self.dropped.load(Ordering::Relaxed),
            errors_text(&self.health),
        )
    }
}

/// ループバックと録音デバイスをまとめて持ち、狙った時刻の前後が録れたものから調べる
struct Capturing {
    dir: PathBuf,
    streams: Vec<CapStream>,
    hits: Vec<Hit>,
    packets_csv: BufWriter<File>,
    targets: Arc<Vec<u64>>,
    look_ns: u64,
    burst_ms: f64,
    next_flush_ns: u64,
}

impl Capturing {
    fn new(dir: &Path, targets: Arc<Vec<u64>>, look_ns: u64, burst_ms: f64) -> Self {
        let mut csv = BufWriter::new(File::create(dir.join("capture_packets.csv")).expect("capture_packets.csv"));
        writeln!(csv, "stream,pos,stored_before,frames,capture_ns,callback_ns,qpc_entry_ns,jump_us,break").unwrap();
        Self { dir: dir.to_path_buf(), streams: vec![], hits: vec![], packets_csv: csv, targets, look_ns, burst_ms, next_flush_ns: 0 }
    }

    /// 録り始める。ループバックは出力デバイスに入力の流れを開く（cpal が AUDCLNT_STREAMFLAGS_LOOPBACK を付ける）
    fn open(&mut self, source: CapSource, info: &DeviceInfo, devs: Vec<u8>) -> Result<(), String> {
        let open_start = qpc_now_ns();
        let (device, cfg, title) = match source {
            CapSource::Loopback(n) => {
                let d = devices::find_output(Some(&info.id)).ok_or("開けない")?;
                let c = d.default_output_config().map_err(|e| format!("default_output_config: {e}"))?;
                (d, c, format!("デバイス {n} のループバック（{}）", info.name))
            }
            CapSource::Input(n) => {
                let d = find_input(&info.id).ok_or("開けない")?;
                let c = d.default_input_config().map_err(|e| format!("default_input_config: {e}"))?;
                (d, c, format!("録音デバイス {n}（{}）", info.name))
            }
        };
        let fmt = StreamFormat { rate: cfg.sample_rate(), channels: cfg.channels() as usize, format: cfg.sample_format() };
        let (sp, sc) = rtrb::RingBuffer::<f32>::new((fmt.rate as usize * 4).max(1 << 16));
        let (pp, pc) = rtrb::RingBuffer::<CapPacket>::new(4096);
        let dropped = Arc::new(AtomicU64::new(0));
        let health = Arc::new(Health::default());
        let feed = CaptureFeed { samples: sp, packets: pp, pos: 0, stored: 0, dropped: dropped.clone() };
        let config =
            cpal::StreamConfig { channels: fmt.channels as u16, sample_rate: fmt.rate, buffer_size: cpal::BufferSize::Default };
        let ch = fmt.channels;
        let stream = match fmt.format {
            SampleFormat::F32 => build_capture::<f32>(&device, &config, ch, feed, health.clone()),
            SampleFormat::I16 => build_capture::<i16>(&device, &config, ch, feed, health.clone()),
            SampleFormat::I32 => build_capture::<i32>(&device, &config, ch, feed, health.clone()),
            SampleFormat::U16 => build_capture::<u16>(&device, &config, ch, feed, health.clone()),
            SampleFormat::F64 => build_capture::<f64>(&device, &config, ch, feed, health.clone()),
            other => Err(format!("扱っていない形式：{other:?}")),
        }?;
        stream.play().map_err(|e| format!("play: {e}"))?;
        let open_done = qpc_now_ns();
        let q1 = qpc_now_ns();
        let now = instant_ns(stream.now());
        let q2 = qpc_now_ns();
        self.streams.push(CapStream {
            source,
            label: source.label(),
            title,
            id: info.id.clone(),
            fmt,
            stream: Some(stream),
            health,
            dropped,
            samples: sc,
            packets: pc,
            track: CaptureTrack::new(fmt.rate),
            next_click: 0,
            devs,
            open_ns: open_done.saturating_sub(open_start),
            now_diff_ns: ((q1 / 2 + q2 / 2) as i64) - now as i64,
        });
        Ok(())
    }

    /// 輪から取り出し、狙った時刻の前後が録れた回を調べる。`finished` なら、録れていない回も「録れていない」で閉じる
    fn tick(&mut self, events: &[ClickEvent], finished: bool) {
        let targets = self.targets.clone();
        let (look, burst_ms) = (self.look_ns, self.burst_ms);
        let mut new_hits = false;
        for (si, s) in self.streams.iter_mut().enumerate() {
            // パケット → 標本 の順に取り出す（コールバックは標本を先に書くので、取り出したパケットの標本は必ずそろっている）
            while let Ok(p) = s.packets.pop() {
                let (jump, brk) = match s.track.add_packet(p) {
                    PacketLink::First => (String::new(), 0),
                    PacketLink::Connected(j) => (format!("{:.1}", j / 1e3), 0),
                    PacketLink::Broken(j) => (format!("{:.1}", j / 1e3), 1),
                };
                let _ = writeln!(
                    self.packets_csv,
                    "{},{},{},{},{},{},{},{jump},{brk}",
                    s.label, p.pos, p.stored_before, p.frames, p.capture_ns, p.callback_ns, p.qpc_ns
                );
            }
            let n = s.samples.slots();
            if n > 0 {
                if let Ok(chunk) = s.samples.read_chunk(n) {
                    let (a, b) = chunk.as_slices();
                    s.track.add_samples(a);
                    s.track.add_samples(b);
                    chunk.commit_all();
                }
            }
            while s.next_click < targets.len() {
                let t = targets[s.next_click];
                let results: Vec<(u8, Result<(f64, Onset), HitMiss>)> = match s.track.window(t.saturating_sub(look), t + look) {
                    Err(WindowMiss::NotYet) if !finished => break,
                    Err(WindowMiss::NotYet) | Err(WindowMiss::NotCaptured) => {
                        s.devs.iter().map(|&d| (d, Err(HitMiss::NotCaptured))).collect()
                    }
                    Err(WindowMiss::Gap) => s.devs.iter().map(|&d| (d, Err(HitMiss::Gap))).collect(),
                    Ok((win, start)) => {
                        let mut v = vec![];
                        for &d in &s.devs {
                            let r = find_click_onset(&win, s.fmt.rate, click_freq_for(d), burst_ms).and_then(|o| {
                                s.track.pos_to_ns(start as f64 + o.pos).map(|ns| (ns, o)).ok_or(HitMiss::Gap)
                            });
                            v.push((d, r));
                        }
                        v
                    }
                };
                for (d, r) in results {
                    self.hits.push(Hit { stream: si, dev: d, click: s.next_click, res: r });
                }
                new_hits = true;
                s.next_click += 1;
            }
            // 次に調べる回の窓より前は要らない（長く流しても手元が増えない）
            let keep = targets.get(s.next_click).map(|t| t.saturating_sub(look + 500_000_000)).unwrap_or(u64::MAX);
            s.track.trim_before(keep);
        }
        if new_hits || finished {
            self.write_clicks_csv(events);
        }
        let now = qpc_now_ns();
        if finished || now >= self.next_flush_ns {
            let _ = self.packets_csv.flush();
            self.next_flush_ns = now + 1_000_000_000;
        }
    }

    fn stop(&mut self) {
        for s in self.streams.iter_mut() {
            drop(s.stream.take());
        }
    }

    fn rows(&self, events: &[ClickEvent]) -> Vec<ClickRow> {
        let sources: Vec<CapSource> = self.streams.iter().map(|s| s.source).collect();
        click_rows(&self.hits, &sources, &self.targets, events)
    }

    /// loopback_clicks.csv を書き直す（基準との差は、ほかの流れの同じ回がそろってから入る）
    fn write_clicks_csv(&self, events: &[ClickEvent]) {
        let Ok(f) = File::create(self.dir.join("loopback_clicks.csv")) else { return };
        let mut w = BufWriter::new(f);
        let first = self.targets.first().copied().unwrap_or(0);
        let _ = writeln!(
            w,
            "stream,dev,open,click,status,target_ns,target_rel_ms,onset_ns,onset_minus_target_us,minus_base_us,minus_lb1_us,aimed_minus_target_us,peak_dbfs,snr_db,width_ms"
        );
        for r in self.rows(events) {
            let found = r.res.ok();
            let _ = writeln!(
                w,
                "{},{},{},{},{},{},{:.3},{},{},{},{},{},{},{},{}",
                self.streams[r.stream].label,
                r.dev,
                r.open_no,
                r.click,
                r.status(),
                r.target_ns,
                (r.target_ns as i64 - first as i64) as f64 / 1e6,
                found.map(|(ns, _)| format!("{ns:.0}")).unwrap_or_default(),
                opt_us(found.map(|(ns, _)| ns - r.target_ns as f64)),
                opt_us(r.minus_base_ns),
                opt_us(r.minus_lb1_ns),
                opt_us(r.aimed_minus_target_ns),
                found.map(|(_, o)| format!("{:.1}", 20.0 * (o.peak as f64).log10())).unwrap_or_default(),
                found.map(|(_, o)| format!("{:.1}", o.snr_db)).unwrap_or_default(),
                found.map(|(_, o)| format!("{:.3}", o.width_ms)).unwrap_or_default(),
            );
        }
        let _ = w.flush();
    }

    /// 要約の「録った流れ」と「結果」
    fn report(&self, events: &[ClickEvent], join: bool) -> String {
        let rows = self.rows(events);
        let mut s = String::new();
        s.push_str("==== 録った流れ\n");
        for c in &self.streams {
            s.push_str(&c.header());
        }
        s.push_str("\n==== 結果（ms。＋は遅い。回は 1 から。鳴らした回だけ数える）\n");
        for (si, c) in self.streams.iter().enumerate() {
            for &d in &c.devs {
                let mine: Vec<&ClickRow> = rows.iter().filter(|r| r.stream == si && r.dev == d).collect();
                let st = section_stats(mine.iter().copied());
                let base_mark = match c.source {
                    CapSource::Loopback(1) => "（基準）",
                    CapSource::Input(_) if d == 1 => "（この流れの基準）",
                    _ => "",
                };
                let join_mark = if join && d == 2 { "・途中参加：回ごとに開き直し（差は開き直しごと）" } else { "" };
                s.push_str(&format!("-- {}：{} の {} Hz{base_mark}{join_mark}\n", c.label, c.title, click_freq_for(d)));
                s.push_str(&stats_line("狙いとの差", &st.vs_target));
                let is_base = matches!(c.source, CapSource::Loopback(1)) || (matches!(c.source, CapSource::Input(_)) && d == 1);
                if !is_base {
                    s.push_str(&stats_line("基準との差", &st.vs_base));
                }
                if matches!(c.source, CapSource::Input(_)) {
                    s.push_str(&stats_line("lb1 との差", &st.vs_lb1));
                }
                let misses: Vec<String> = HitMiss::ALL
                    .iter()
                    .filter_map(|m| st.misses.get(m).map(|n| format!("{} {n}", m.text())))
                    .collect();
                s.push_str(&format!(
                    "  取れなかった：{}・鳴らしていない {} 回{}\n",
                    if misses.is_empty() { "0 回".to_string() } else { misses.join("・") },
                    st.not_played,
                    if st.found_unplayed > 0 { format!("（うち {} 回は鳴らしていないのに見つかった＝見分けの誤りの疑い）", st.found_unplayed) } else { String::new() }
                ));
                s.push_str("  回・狙いとの差・基準との差・lb1 との差・状態\n");
                const MAX_LINES: usize = 200;
                for r in mine.iter().take(MAX_LINES) {
                    s.push_str(&format!(
                        "  {:>4}・{}・{}・{}・{}\n",
                        r.click + 1,
                        opt_ms(r.minus_target_ns()),
                        opt_ms(r.minus_base_ns),
                        opt_ms(r.minus_lb1_ns),
                        r.status()
                    ));
                }
                if mine.len() > MAX_LINES {
                    s.push_str(&format!("  （残り {} 回は loopback_clicks.csv）\n", mine.len() - MAX_LINES));
                }
                s.push('\n');
            }
        }
        s
    }
}

fn build_capture<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut feed: CaptureFeed,
    health: Arc<Health>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let ch = channels.max(1);
    let data_cb = move |data: &[T], info: &cpal::InputCallbackInfo| {
        let qpc = qpc_now_ns();
        let ts = info.timestamp();
        let frames = data.len() / ch;
        let mono = data.chunks_exact(ch).map(|f| f.iter().map(|&v| <f32 as FromSample<T>>::from_sample_(v)).sum::<f32>() / ch as f32);
        feed.push(frames, mono, instant_ns(ts.capture), instant_ns(ts.callback), qpc);
    };
    let err_cb = move |e: cpal::Error| {
        if let Ok(mut v) = health.errors.lock() {
            if v.len() < 100 {
                v.push(format!("{:?}: {e}", e.kind()));
            }
        }
    };
    device.build_input_stream::<T, _, _>(*config, data_cb, err_cb, None).map_err(|e| format!("build_input_stream: {e}"))
}

// ----------------------------------------------------------------------
// slip_probe
// ----------------------------------------------------------------------

/// 輪から読んで流すだけ（足りなければ無音にして数える）
struct RingPlayer {
    cons: rtrb::Consumer<f32>,
    started: bool,
    health: Arc<Health>,
}

impl Render for RingPlayer {
    fn render(&mut self, out: &mut [f32], t: &CbTiming) {
        let ch = t.channels.max(1);
        let mut n = out.len().min(self.cons.slots());
        n -= n % ch;
        if n > 0 {
            if let Ok(chunk) = self.cons.read_chunk(n) {
                let (a, b) = chunk.as_slices();
                for (d, s) in out.iter_mut().zip(a.iter().chain(b.iter())) {
                    *d = *s;
                }
                chunk.commit_all();
                self.started = true;
            }
        }
        if n < out.len() && self.started {
            self.health.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

const SLIP_CHUNK: usize = 1024;

/// 13.1 の 4：1つのデバイスに 1kHz の正弦波を rubato の Slip（倍率固定）に通して流す
#[test]
#[ignore]
fn slip_probe() {
    let picked = pick_devices(1, 1, "slip_probe", "");
    let p = picked.into_iter().next().unwrap();
    let secs = env_f64_in("PL_PROBE_SECS", 60.0, 1.0, MAX_PROBE_SECS);
    let ratio = env_f64_in("PL_PROBE_SLIP_RATIO", 1.0001, 0.99, 1.01);
    let alternate = env_f64_in("PL_PROBE_SLIP_ALTERNATE_SEC", 0.0, 0.0, 3_600.0);
    let hz = env_f64_in("PL_PROBE_SINE_HZ", 1000.0, 20.0, 20_000.0);
    let db = env_f64_in("PL_PROBE_SINE_DB", -20.0, f64::MIN, f64::MAX);
    let amp = level_amp("PL_PROBE_SINE_DB", db).unwrap_or_else(|e| panic!("{e}"));
    let dir = make_run_dir();
    let started = local_stamp();
    println!("記録の置き場所：{}", dir.display());
    let device = devices::find_output(Some(&p.info.id)).unwrap_or_else(|| panic!("開けない：{}", p.info.name));
    let fmt = default_format(&device).unwrap_or_else(|e| panic!("{e}"));
    let ch = fmt.channels;

    let (mut prod, cons) = rtrb::RingBuffer::<f32>::new((fmt.rate as usize / 4).max(4096) * ch);
    let mut slip = Slip::<f32>::new(SLIP_CHUNK, ch, FixedAsync::Input).expect("Slip::new");
    slip.set_resample_ratio(ratio, false)
        .unwrap_or_else(|e| panic!("PL_PROBE_SLIP_RATIO={ratio} は Slip の範囲の外：{e}"));
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let rate = fmt.rate;
    let chunks_path = dir.join("slip_chunks.csv");
    let gen = std::thread::spawn(move || {
        let mut w = BufWriter::new(File::create(chunks_path).expect("slip_chunks.csv"));
        let _ = writeln!(w, "chunk,t_s,mode,in_frames,out_frames,slipped");
        let mut sine = SineGen::new(hz, rate, amp);
        let mut inbuf = vec![0f32; (SLIP_CHUNK + 64) * ch];
        let mut outbuf = vec![0f32; slip.output_frames_max() * ch];
        let (mut out_total, mut chunk_no) = (0u64, 0u64);
        let (mut slipped_total, mut in_slip_total) = (0i64, 0u64);
        while !stop2.load(Ordering::Relaxed) {
            let on = slip_on_at(out_total, rate, alternate);
            let need_out = if on { slip.output_frames_next() } else { SLIP_CHUNK };
            if prod.slots() < need_out * ch {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            let in_n = if on { slip.input_frames_next() } else { SLIP_CHUNK };
            if inbuf.len() < in_n * ch {
                inbuf.resize(in_n * ch, 0.0);
            }
            sine.fill(&mut inbuf[..in_n * ch], ch);
            let out_n = if on {
                let out_cap = outbuf.len() / ch;
                let ia = InterleavedSlice::new(&inbuf[..in_n * ch], ch, in_n).expect("in");
                let mut oa = InterleavedSlice::new_mut(&mut outbuf[..], ch, out_cap).expect("out");
                let (_used, produced) = slip.process_into_buffer(&ia, &mut oa, None).expect("Slip");
                in_slip_total += in_n as u64;
                produced
            } else {
                outbuf[..in_n * ch].copy_from_slice(&inbuf[..in_n * ch]);
                in_n
            };
            let (pushed, _) = prod.push_partial_slice(&outbuf[..out_n * ch]);
            debug_assert_eq!(pushed.len(), out_n * ch);
            let slipped = out_n as i64 - in_n as i64;
            slipped_total += slipped.abs();
            let _ = writeln!(
                w,
                "{chunk_no},{:.4},{},{in_n},{out_n},{slipped}",
                out_total as f64 / rate as f64,
                if on { "slip" } else { "bypass" }
            );
            out_total += out_n as u64;
            chunk_no += 1;
            // 約1秒ごとに書き出す（途中で止めても残るように）
            if chunk_no % ((rate as u64 / SLIP_CHUNK as u64).max(1)) == 0 {
                let _ = w.flush();
            }
        }
        let _ = w.flush();
        (slipped_total, in_slip_total, out_total)
    });
    // 輪が埋まるまで待ってから鳴らし始める
    std::thread::sleep(Duration::from_millis(300));
    let mut sink = CbSink::new(&dir);
    let health = Arc::new(Health::default());
    let log = sink.new_log();
    let opened = open_stream(&device, fmt, 1, 1, Box::new(RingPlayer { cons, started: false, health: health.clone() }), log, health.clone())
        .unwrap_or_else(|e| panic!("{}：{e}", p.info.name));
    println!(
        "{}（{} Hz・{} ch・{:?}）へ {hz} Hz・{db} dBFS を {secs} 秒。Slip の倍率 {ratio}{}",
        p.info.name,
        fmt.rate,
        fmt.channels,
        fmt.format,
        if alternate > 0.0 { format!("・{alternate} 秒ごとに通す／通さないを交互") } else { String::new() }
    );
    let mut run = DevRun { picked: p, opened, health };
    // 要約の頭（設定）。始める前に一度書いておく（途中で止めても、設定とデバイスは残る）
    let mut head = String::new();
    head.push_str("同時再生の計測：slip_probe（rubato::Slip の試聴）\n");
    head.push_str(&format!("開始：{started}（ローカル時刻）\n"));
    head.push_str(&format!(
        "正弦波 {hz} Hz・{db} dBFS・{secs} 秒・Slip の倍率 {ratio}（塊 {SLIP_CHUNK} フレームを入力）{}\n\n",
        if alternate > 0.0 { format!("・{alternate} 秒ごとに通す／通さないを交互（最初は通す）") } else { "・ずっと通す".into() }
    ));
    head.push_str(&device_header(&run));
    {
        let mut e = head.clone();
        e.push_str("\n※途中の要約（流している途中か、途中で止めた）。終わると全体の要約で書き直す。\n");
        e.push_str("　滑らせた所は slip_chunks.csv（約1秒ごとに書き出し）、コールバックの記録は callbacks.csv（1秒ごと）に残っている。\n");
        let _ = fs::write(dir.join("slip_summary.txt"), &e);
    }
    let t0 = qpc_now_ns();
    let mut next_flush = t0;
    wait_until(t0 + (secs * 1e9) as u64, || {
        sink.drain();
        if qpc_now_ns() >= next_flush {
            sink.flush();
            next_flush = qpc_now_ns() + 1_000_000_000;
        }
    });
    drop(run.opened.stream.take());
    stop.store(true, Ordering::Relaxed);
    let (slipped_total, in_slip_total, out_total) = gen.join().expect("generator");
    sink.drain();
    sink.flush();

    let expected = (ratio - 1.0).abs() * in_slip_total as f64;
    let mut s = head;
    s.push_str(&format!(
        "  滑らせた（足した／抜いた）フレーム：{slipped_total}（倍率からの見込み {expected:.1}）・作った総フレーム {out_total}\n"
    ));
    s.push_str(&format!(
        "  音切れ（輪が空だった回）：{}・記録の取りこぼし：{} 件・エラー：{}\n\n",
        run.health.underruns.load(Ordering::Relaxed),
        run.health.dropped_records.load(Ordering::Relaxed),
        errors_text(&run.health)
    ));
    s.push_str("聞き方：slip_chunks.csv の slipped が 0 でない時刻（t_s。鳴り始めからの目安）に、にごりやプチッという音が分かるか。\n");
    let _ = fs::write(dir.join("slip_summary.txt"), &s);
    println!("{s}");
    println!("記録：{}", dir.display());
}

// ======================================================================
// 純粋な関数のテスト（#[ignore] なし。音は出さない）
// ======================================================================

fn dev(id: &str, name: &str, is_default: bool) -> DeviceInfo {
    DeviceInfo { id: id.into(), name: name.into(), is_default, kind: "speaker" }
}

#[test]
fn qpc_conversion_matches_cpal_grid() {
    // 10MHz：1カウント＝100ns
    assert_eq!(qpc_to_ns(12_345, 10_000_000), 1_234_500);
    // 3MHz：100ns 単位に切り捨ててから ns
    // 1.000000333… 秒 → 100ns 単位で切り捨てて 1,000,000,300ns
    assert_eq!(qpc_to_ns(3_000_001, 3_000_000), 1_000_000_300);
    assert_eq!(qpc_to_ns(3_000_000 + 30, 3_000_000), 1_000_010_000);
    assert_eq!(qpc_to_ns(0, 10_000_000), 0);
    assert_eq!(qpc_to_ns(5, 0), 0);
}

#[test]
fn fit_line_recovers_slope_and_intercept() {
    let pts: Vec<(f64, f64)> = (0..100).map(|i| (i as f64, 3.5 * i as f64 + 7.0)).collect();
    let l = fit_line(pts.iter().copied()).unwrap();
    assert!((l.slope - 3.5).abs() < 1e-12);
    assert!((l.intercept - 7.0).abs() < 1e-9);
    assert!(fit_line(std::iter::once((1.0, 1.0))).is_none());
    assert!(fit_line([(2.0, 1.0), (2.0, 5.0)].into_iter()).is_none());
}

#[test]
fn residual_stats_of_known_noise() {
    let line = Line { slope: 1.0, intercept: 0.0 };
    // 残りが +1, -1 を交互 → 標準偏差 1・最大 1
    let pts: Vec<(f64, f64)> = (0..10).map(|i| (i as f64, i as f64 + if i % 2 == 0 { 1.0 } else { -1.0 })).collect();
    let st = residual_stats(pts.into_iter(), &line);
    assert_eq!(st.n, 10);
    assert!((st.std - 1.0).abs() < 1e-12);
    assert!((st.max_abs - 1.0).abs() < 1e-12);
    assert_eq!(residual_stats(std::iter::empty(), &line), ResidualStats::default());
}

#[test]
fn ppm_and_drift_per_hour() {
    // 48kHz ちょうど → 0 ppm
    assert!(ppm_from_slope(1e9 / 48_000.0, 48_000).abs() < 1e-6);
    // 実のレートが 48000 × (1 + 50e-6) → +50 ppm
    let actual = 48_000.0 * (1.0 + 50e-6);
    assert!((ppm_from_slope(1e9 / actual, 48_000) - 50.0).abs() < 1e-6);
    assert!((drift_ms_per_hour(1.0) - 3.6).abs() < 1e-12);
    assert!((drift_ms_per_hour(-10.0) + 36.0).abs() < 1e-12);
}

#[test]
fn stall_is_playback_not_advancing() {
    assert!(!is_stalled(None, 100));
    assert!(!is_stalled(Some(100), 101));
    assert!(is_stalled(Some(100), 100));
    assert!(is_stalled(Some(100), 99));
}

/// 名目 48kHz・実 +40ppm・L 12ms・のこぎり形（±0.5ms）のコールバックを作って、分析が戻すか
fn synth_records(dev: u8, ppm: f64, l_ns: u64, saw_ns: f64, stall_every: u64, n: u64) -> Vec<CbRecord> {
    let rate = 48_000.0;
    let actual = rate * (1.0 + ppm * 1e-6);
    let frames = 480u64;
    let t0 = 5_000_000_000u64;
    let mut out = vec![];
    let mut prev: Option<u64> = None;
    for seq in 0..n {
        let wb = seq * frames;
        // 書いた位置が鳴る時刻（＋のこぎり形）
        let ideal = t0 as f64 + l_ns as f64 + wb as f64 * 1e9 / actual;
        // のこぎり形は 0〜saw_ns（実物でも playback は callback＋L より前に来ない）
        let saw = saw_ns * ((seq % 50) as f64 / 50.0);
        let callback = t0 + (wb as f64 * 1e9 / actual) as u64;
        let mut playback = if seq == 0 { callback + l_ns } else { (ideal + saw) as u64 };
        if stall_every > 0 && seq > 0 && seq % stall_every == 0 {
            playback = prev.unwrap();
        }
        let stalled = is_stalled(prev, playback);
        prev = Some(prev.map_or(playback, |p| p.max(playback)));
        out.push(CbRecord {
            dev,
            open_no: 1,
            seq,
            frames: frames as u32,
            written_before: wb,
            callback_ns: callback,
            playback_ns: playback,
            qpc_ns: callback + 30_000,
            stalled,
        });
    }
    out
}

#[test]
fn analyze_drift_recovers_ppm_l_and_stalls() {
    let recs = synth_records(1, 40.0, 12_000_000, 1_000_000.0, 997, 20_000);
    let a = analyze_drift(&recs, 48_000, 200_000_000).unwrap();
    assert_eq!(a.l_ns, 12_000_000);
    assert!((a.ppm.unwrap() - 40.0).abs() < 0.5, "ppm {:?}", a.ppm);
    assert!(a.stalls >= 19 && a.stalls <= 21, "stalls {}", a.stalls);
    // のこぎり形 0〜1ms → 線のまわりで ±0.5ms 前後、標準偏差は 0.29ms 前後
    assert!(a.resid.max_abs > 400_000.0 && a.resid.max_abs < 700_000.0, "{:?}", a.resid);
    assert!(a.resid.std > 200_000.0 && a.resid.std < 400_000.0, "{:?}", a.resid);
    assert!(a.buffered_min_ns > -50_000);
    assert_eq!(a.entry_minus_cb_median_ns, 30_000);
    assert_eq!(a.gaps_over_3x, 0);
    assert!(a.early.is_some());
    // 2台の差：+40 と -10 → 50ppm → 1時間で 180ms
    let b = analyze_drift(&synth_records(2, -10.0, 20_000_000, 0.0, 0, 20_000), 48_000, 200_000_000).unwrap();
    let diff = b.ppm.unwrap() - a.ppm.unwrap();
    assert!((drift_ms_per_hour(diff) + 180.0).abs() < 2.0, "{diff}");
    assert_eq!(b.stalls, 0);
}

#[test]
fn residual_rows_are_in_time_order_and_skip_start() {
    let recs = synth_records(1, 0.0, 10_000_000, 0.0, 0, 200);
    let line = fit_line(drift_points(&recs, 100_000_000).into_iter()).unwrap();
    let rows = residual_rows(&recs, &line, 100_000_000);
    assert!(!rows.is_empty());
    assert!(rows.windows(2).all(|w| w[0].1 <= w[1].1));
    assert!(rows[0].1 >= 0.1 - 1e-9);
    assert!(rows.iter().all(|r| r.3.abs() < 1.0), "residual µs");
}

#[test]
fn click_burst_shape() {
    let b = click_burst(1000.0, 48_000, 5.0, 0.5, 0.5);
    assert_eq!(b.len(), 240);
    let peak = b.iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 0.5 + 1e-6 && peak > 0.45, "peak {peak}");
    // 両端は弱めてある
    assert!(b[0].abs() < 0.01 && b[239].abs() < 0.05, "{} {}", b[0], b[239]);
    // 1kHz：48 標本で1周
    assert!((b[60] - b[108]).abs() < 1e-4);
    assert_eq!(click_freq_for(1), 1000.0);
    assert_eq!(click_freq_for(2), 3000.0);
    assert_eq!(click_freq_for(3), 5000.0);
    assert!((db_to_amp(-20.0) - 0.1).abs() < 1e-6);
}

#[test]
fn click_targets_and_burst_starts() {
    let t = click_targets(1_000, &[0.0, 10.0], 3, 2000.0);
    assert_eq!(t, vec![1_000, 2_000_001_000, 4_000_001_000, 10_000_001_000, 12_000_001_000, 14_000_001_000]);
    // 重なりは1つに
    assert_eq!(click_targets(0, &[0.0, 2.0], 2, 2000.0), vec![0, 2_000_000_000, 4_000_000_000]);
    assert_eq!(click_burst_starts(None, None).unwrap(), vec![0.0]);
    assert_eq!(click_burst_starts(None, Some(3600.0)).unwrap(), vec![0.0, 1800.0, 3600.0]);
    assert_eq!(click_burst_starts(Some("3600, 0,1800"), Some(1.0)).unwrap(), vec![0.0, 1800.0, 3600.0]);
    assert!(click_burst_starts(Some("a"), None).is_err());
    assert!(click_burst_starts(Some("-1"), None).is_err());
}

#[test]
fn aiming_frame_from_line() {
    // 48kHz・y は playback − 最初の playback（ns）
    let slope = 1e9 / 48_000.0;
    let line = Line::through(4800.0, 100_000_000.0, slope);
    // 4800 フレーム目が 100ms → 1秒後は 4800 + 48000 フレーム目
    assert_eq!(aim_frame(&line, 1_100_000_000.0), 52_800);
    // 丸め：半フレーム手前
    assert_eq!(aim_frame(&line, 100_000_000.0 + slope * 0.4), 4800);
    // 決め方：まだ・今・遅れた
    assert_eq!(decide_click(&line, 1_100_000_000.0, 0, 480, 2400), Decide::NotYet);
    assert_eq!(decide_click(&line, 1_100_000_000.0, 52_000, 480, 2400), Decide::Now { start_frame: 52_800 });
    assert_eq!(decide_click(&line, 1_100_000_000.0, 53_000, 480, 2400), Decide::Late { start_frame: 52_800 });
    // L を引くと、同じ狙いでも L ぶん後ろのフレーム（狙いを origin = first_pb − L で測るため）
    let l_ns = 10_000_000.0;
    assert_eq!(aim_frame(&line, 1_100_000_000.0 + l_ns), 52_800 + 480);
    // 書く範囲のどこにクリックが入るか
    assert_eq!(burst_index(52_800, 52_800, 240), Some(0));
    assert_eq!(burst_index(53_039, 52_800, 240), Some(239));
    assert_eq!(burst_index(53_040, 52_800, 240), None);
    assert_eq!(burst_index(52_799, 52_800, 240), None);
}

#[test]
fn fit_window_keeps_recent_points_without_growing() {
    let mut w = FitWindow::new(8, 1_000);
    let cap0 = w.pts.capacity();
    for i in 0..20u64 {
        w.push(i as f64, 2.0 * i as f64 + 1.0, i * 100);
    }
    // 容量 8・窓 1000ns（100ns おき）→ 8 点まで
    assert!(w.len() <= 8);
    assert_eq!(w.pts.capacity(), cap0, "容量が増えた");
    let l = w.fit().unwrap();
    assert!((l.slope - 2.0).abs() < 1e-9);
    assert_eq!(w.latest(), Some((19.0, 39.0)));
    // 時間の窓で古い点が落ちる
    let mut w2 = FitWindow::new(100, 250);
    for i in 0..10u64 {
        w2.push(i as f64, i as f64, i * 100);
    }
    assert!(w2.len() <= 4, "len {}", w2.len());
}

#[test]
fn sine_generator_is_continuous_across_chunks() {
    let mut a = SineGen::new(1000.0, 48_000, 0.1);
    let mut whole = vec![0f32; 960 * 2];
    a.fill(&mut whole, 2);
    let mut b = SineGen::new(1000.0, 48_000, 0.1);
    let mut p1 = vec![0f32; 333 * 2];
    let mut p2 = vec![0f32; 627 * 2];
    b.fill(&mut p1, 2);
    b.fill(&mut p2, 2);
    let joined: Vec<f32> = p1.into_iter().chain(p2).collect();
    for (x, y) in whole.iter().zip(joined.iter()) {
        assert!((x - y).abs() < 1e-6);
    }
    assert_eq!(whole[0], whole[1]);
    assert!(whole.iter().all(|v| v.abs() <= 0.1 + 1e-6));
}

#[test]
fn slip_segments_alternate() {
    assert!(slip_on_at(10_000_000, 48_000, 0.0));
    assert!(slip_on_at(0, 48_000, 10.0));
    assert!(slip_on_at(48_000 * 10 - 1, 48_000, 10.0));
    assert!(!slip_on_at(48_000 * 10, 48_000, 10.0));
    assert!(slip_on_at(48_000 * 20, 48_000, 10.0));
}

/// Slip を倍率 1.0001 で通すと、入力 1,000,000 フレームあたり約 100 フレーム足す（slip_probe の見込みの式）
#[test]
fn slip_ratio_adds_expected_frames() {
    let ch = 2;
    let mut slip = Slip::<f32>::new(SLIP_CHUNK, ch, FixedAsync::Input).unwrap();
    slip.set_resample_ratio(1.0001, false).unwrap();
    let mut sine = SineGen::new(1000.0, 48_000, 0.1);
    let mut inbuf = vec![0f32; SLIP_CHUNK * ch];
    let mut outbuf = vec![0f32; slip.output_frames_max() * ch];
    let (mut tin, mut tout) = (0usize, 0usize);
    while tin < 1_000_000 {
        let n = slip.input_frames_next();
        sine.fill(&mut inbuf[..n * ch], ch);
        let ia = InterleavedSlice::new(&inbuf[..n * ch], ch, n).unwrap();
        let cap = outbuf.len() / ch;
        let mut oa = InterleavedSlice::new_mut(&mut outbuf[..], ch, cap).unwrap();
        let (used, produced) = slip.process_into_buffer(&ia, &mut oa, None).unwrap();
        tin += used;
        tout += produced;
    }
    let added = tout as i64 - tin as i64;
    assert!((95..=105).contains(&added), "added {added}");
}

#[test]
fn select_devices_by_various_forms() {
    let list = vec![
        dev("{a}", "スピーカー (Realtek Audio)", true),
        dev("{b}", "ヘッドホン (USB Audio)", false),
        dev("{c}", "スピーカー (HDMI)", false),
    ];
    assert_eq!(select_devices(&list, "default").unwrap(), vec![0]);
    assert_eq!(select_devices(&list, "#2|#0").unwrap(), vec![2, 0]);
    assert_eq!(select_devices(&list, "{b}").unwrap(), vec![1]);
    assert_eq!(select_devices(&list, "ヘッドホン (USB Audio)").unwrap(), vec![1]);
    assert_eq!(select_devices(&list, " usb | hdmi ").unwrap(), vec![1, 2]);
    // 名前の一部が2つに当たる
    assert!(select_devices(&list, "スピーカー").unwrap_err().contains("複数"));
    // 同じデバイスを2回
    assert!(select_devices(&list, "default|#0").unwrap_err().contains("2回"));
    assert!(select_devices(&list, "#9").is_err());
    assert!(select_devices(&list, "無い").is_err());
    assert!(select_devices(&list, " | ").is_err());
    assert!(device_list_text(&list).contains("[既定]"));
}

#[test]
fn folder_stamp_and_base_dir() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    // 2026-11-15 01:37:05（UTC として）
    let unix = 1_794_706_625u64;
    let ft = unix * 10_000_000 + 116_444_736_000_000_000;
    assert_eq!(stamp_from_filetime(ft), "20261115-013705");
    // 2024-02-29 23:59:59
    let ft2 = 1_709_251_199u64 * 10_000_000 + 116_444_736_000_000_000;
    assert_eq!(stamp_from_filetime(ft2), "20240229-235959");

    assert_eq!(probe_base_dir(Some(r"D:\probe"), Some(r"C:\Users\x")).unwrap(), PathBuf::from(r"D:\probe"));
    assert_eq!(
        probe_base_dir(None, Some(r"C:\Users\x")).unwrap(),
        PathBuf::from(r"C:\Users\x").join("Documents").join("pl-player-probe")
    );
    assert_eq!(
        probe_base_dir(Some("  "), Some(r"C:\Users\x")).unwrap(),
        PathBuf::from(r"C:\Users\x").join("Documents").join("pl-player-probe")
    );
    assert!(probe_base_dir(None, None).is_err());
}

#[test]
fn median_and_empty() {
    assert_eq!(median_i64(vec![]), None);
    assert_eq!(median_i64(vec![5, 1, 3]), Some(3));
}

#[test]
fn level_is_capped_and_bad_numbers_are_refused() {
    // -20 を 20 と打った・0 dBFS・NaN・無限は断る（切り詰めて鳴らさない）
    assert!(level_amp("PL_PROBE_SINE_DB", 20.0).is_err());
    assert!(level_amp("PL_PROBE_SINE_DB", 0.0).is_err());
    assert!(level_amp("PL_PROBE_SINE_DB", -5.9).is_err());
    assert!(level_amp("PL_PROBE_SINE_DB", f64::NAN).is_err());
    assert!(level_amp("PL_PROBE_SINE_DB", f64::INFINITY).is_err());
    assert!(level_amp("PL_PROBE_SINE_DB", -200.0).is_err());
    assert!((level_amp("PL_PROBE_SINE_DB", -20.0).unwrap() - 0.1).abs() < 1e-6);
    assert!(level_amp("PL_PROBE_CLICK_DB", -6.0).is_ok());
    // 長さ：無限・NaN・負・上限より大きい値は断る（ns に直してもあふれない範囲だけ通す）
    assert!(check_range("PL_PROBE_SECS", f64::INFINITY, 1.0, MAX_PROBE_SECS).is_err());
    assert!(check_range("PL_PROBE_SECS", f64::NAN, 1.0, MAX_PROBE_SECS).is_err());
    assert!(check_range("PL_PROBE_SECS", -1.0, 1.0, MAX_PROBE_SECS).is_err());
    assert!(check_range("PL_PROBE_SECS", 1e30, 1.0, MAX_PROBE_SECS).is_err());
    assert_eq!(check_range("PL_PROBE_SECS", 3600.0, 1.0, MAX_PROBE_SECS), Ok(3600.0));
    assert!(((MAX_PROBE_SECS * 1e9) as u64).checked_add(u64::MAX / 2).is_some());
    assert!(click_burst_starts(Some("1e30"), None).is_err());
    assert!(click_burst_starts(Some("inf"), None).is_err());
    assert!(click_burst_starts(None, Some(f64::INFINITY)).is_err());
    assert!(click_burst_starts(None, Some(-5.0)).is_err());
}

#[test]
fn output_folder_inside_repository_is_detected() {
    let repo = Path::new(r"C:\work\pl-player");
    assert!(path_is_inside(Path::new(r"C:\work\pl-player"), repo));
    assert!(path_is_inside(Path::new(r"C:\work\pl-player\src-tauri\probe"), repo));
    assert!(path_is_inside(Path::new(r"c:\WORK\PL-Player\probe"), repo));
    assert!(!path_is_inside(Path::new(r"C:\work\pl-player-probe"), repo));
    assert!(!path_is_inside(Path::new(r"C:\Users\x\Documents\pl-player-probe"), repo));
    assert!(!path_is_inside(Path::new(r"D:\work\pl-player"), repo));
}

// ---- ClickRenderer を偽の時刻で回す（デバイス無し） ----

const T0: u64 = 7_000_000_000;
const FRAMES_PER_CB: usize = 480; // 48kHz で 10ms

/// 偽のストリーム：コールバック i は 10ms おき、書いて未消化の量は 0、playback = callback + L
fn fake_timing(i: u64, l_ns: u64) -> CbTiming {
    let cb = T0 + i * 10_000_000;
    CbTiming {
        written_before: i * FRAMES_PER_CB as u64,
        frames: FRAMES_PER_CB,
        channels: 2,
        callback_ns: cb,
        playback_ns: cb + l_ns,
        qpc_ns: cb + 20_000,
        stalled: false,
        first_callback_ns: T0,
        first_playback_ns: T0 + l_ns,
        l_ns,
    }
}

fn test_renderer(
    targets: Vec<u64>,
    only: Option<usize>,
    gate_ns: Option<u64>,
    subtract_l: bool,
    skip_ns: u64,
) -> (ClickRenderer, rtrb::Consumer<ClickEvent>) {
    let (p, c) = rtrb::RingBuffer::new(64);
    let r = ClickRenderer {
        dev: 1,
        open_no: 1,
        rate: 48_000,
        burst: click_burst(1000.0, 48_000, 5.0, 0.5, 0.5),
        targets: Arc::new(targets),
        only,
        next: 0,
        sched: None,
        win: FitWindow::new(4096, 3_000_000_000),
        aim: AimMode::Fit,
        subtract_l,
        skip_ns,
        gate_ns,
        horizon_frames: 2400,
        events: p,
        health: Arc::new(Health::default()),
    };
    (r, c)
}

/// コールバック from..to を回し、1ch 目の音（from のフレームから）を返す。2ch 目が同じ値かも見る
fn drive(r: &mut ClickRenderer, from: u64, to: u64, l_ns: u64) -> Vec<f32> {
    let mut mono = vec![];
    let mut buf = vec![0f32; FRAMES_PER_CB * 2];
    for i in from..to {
        buf.fill(0.0);
        r.render(&mut buf, &fake_timing(i, l_ns));
        for f in buf.chunks(2) {
            assert_eq!(f[0], f[1]);
            mono.push(f[0]);
        }
    }
    mono
}

fn events(c: &mut rtrb::Consumer<ClickEvent>) -> Vec<ClickEvent> {
    let mut v = vec![];
    while let Ok(e) = c.pop() {
        v.push(e);
    }
    v
}

/// 鳴った範囲（0 でない最初と最後のフレーム）
fn sounding(mono: &[f32], offset: u64) -> Option<(u64, u64)> {
    let first = mono.iter().position(|v| *v != 0.0)?;
    let last = mono.iter().rposition(|v| *v != 0.0)?;
    Some((first as u64 + offset, last as u64 + offset))
}

#[test]
fn click_render_subtracts_l_in_the_right_direction() {
    let l_ns = 10_000_000; // L = 10ms = 480 フレーム
    let target = T0 + 1_000_000_000;
    // L を引く：playback − L = 狙い のフレーム＝ T0 + k/48000 秒 = T0 + 1秒 → k = 48000
    let (mut r, mut c) = test_renderer(vec![target], None, None, true, 200_000_000);
    let mono = drive(&mut r, 0, 120, l_ns);
    let ev = events(&mut c);
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!(ev[0].status, ClickStatus::Ok);
    assert_eq!(ev[0].start_frame, 48_000);
    assert!(ev[0].used_fit && ev[0].fit_points >= 4);
    assert!((ev[0].aimed_ns - target as i64).abs() < 25_000, "aimed {} target {target}", ev[0].aimed_ns);
    assert_eq!(ev[0].l_ns, l_ns);
    let (a, b) = sounding(&mono, 0).unwrap();
    assert!(a >= 48_000 && b < 48_000 + 240, "{a}..{b}");

    // L を引かない：playback = 狙い → T0 + L + k/48000 = T0 + 1秒 → k = 48000 − 480（L ぶん早いフレーム）
    let (mut r, mut c) = test_renderer(vec![target], None, None, false, 200_000_000);
    let mono = drive(&mut r, 0, 120, l_ns);
    let ev = events(&mut c);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].start_frame, 47_520);
    assert!((ev[0].aimed_ns - target as i64).abs() < 25_000);
    let (a, b) = sounding(&mono, 0).unwrap();
    assert!(a >= 47_520 && b < 47_520 + 240, "{a}..{b}");
}

#[test]
fn click_render_only_aims_the_given_click() {
    let targets = vec![T0 + 500_000_000, T0 + 1_000_000_000, T0 + 1_500_000_000];
    let (mut r, mut c) = test_renderer(targets, Some(1), Some(80_000_000), true, 200_000_000);
    let mono = drive(&mut r, 0, 200, 10_000_000);
    let ev = events(&mut c);
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!((ev[0].click, ev[0].status, ev[0].start_frame), (1, ClickStatus::Ok, 48_000));
    let (a, b) = sounding(&mono, 0).unwrap();
    assert!(a >= 48_000 && b < 48_000 + 240, "{a}..{b}");
}

#[test]
fn click_render_gate_skips_click_right_after_open() {
    // 開いて 200ms ＋ 80ms より前（250ms）の回は狙わず、音も出さない
    let targets = vec![T0 + 250_000_000, T0 + 1_000_000_000];
    let (mut r, mut c) = test_renderer(targets, Some(0), Some(80_000_000), true, 200_000_000);
    let mono = drive(&mut r, 0, 120, 10_000_000);
    let ev = events(&mut c);
    assert_eq!(ev.len(), 1, "{ev:?}");
    assert_eq!((ev[0].click, ev[0].status), (0, ClickStatus::TooSoon));
    assert!(sounding(&mono, 0).is_none());
    // 門の後ろちょうど（290ms）なら狙う
    let (mut r, mut c) = test_renderer(vec![T0 + 290_000_000], Some(0), Some(80_000_000), true, 200_000_000);
    let mono = drive(&mut r, 0, 60, 10_000_000);
    let ev = events(&mut c);
    assert_eq!((ev.len(), ev[0].status, ev[0].start_frame), (1, ClickStatus::Ok, 13_920));
    assert!(sounding(&mono, 0).is_some());
}

#[test]
fn click_render_late_click_is_not_played_and_next_one_is() {
    // コールバック 100（1秒の位置）から回し始める：0.5秒の回はもう書いた後 → Late。1.5秒の回は鳴らす
    let targets = vec![T0 + 500_000_000, T0 + 1_500_000_000];
    let (mut r, mut c) = test_renderer(targets, None, None, true, 0);
    let mono = drive(&mut r, 100, 200, 10_000_000);
    let ev = events(&mut c);
    assert_eq!(ev.len(), 2, "{ev:?}");
    assert_eq!((ev[0].click, ev[0].status, ev[0].start_frame), (0, ClickStatus::Late, 24_000));
    assert_eq!((ev[1].click, ev[1].status, ev[1].start_frame), (1, ClickStatus::Ok, 72_000));
    let (a, b) = sounding(&mono, 100 * FRAMES_PER_CB as u64).unwrap();
    assert!(a >= 72_000 && b < 72_000 + 240, "{a}..{b}");
}

// ---- 録った音からクリックを探す（loopback_click_probe。デバイスは使わず、作った波形で） ----

/// -30 dBFS（loopback_click_probe の既定の大きさ）
const AMP_M30: f32 = 0.031_622_776;

/// 種を固定した雑音（-amp〜amp の一様）
fn lcg_noise(n: usize, amp: f32, seed: u64) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((s >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * amp
        })
        .collect()
}

/// `at` の標本から、高さ `freq` のクリック（5ms）を足す
fn add_click(x: &mut [f32], at: usize, freq: f64, rate: u32, amp: f32) {
    for (i, v) in click_burst(freq, rate, 5.0, CLICK_FADE_MS, amp).iter().enumerate() {
        if let Some(d) = x.get_mut(at + i) {
            *d += v;
        }
    }
}

fn add_sine(x: &mut [f32], freq: f64, rate: u32, amp: f32) {
    for (k, v) in x.iter_mut().enumerate() {
        *v += amp * (2.0 * std::f64::consts::PI * freq * k as f64 / rate as f64).sin() as f32;
    }
}

#[test]
fn onset_is_found_where_the_click_starts() {
    for &rate in &[48_000u32, 44_100] {
        for dev in 1..=3u8 {
            let f = click_freq_for(dev);
            let mut x = vec![0f32; rate as usize * 2 / 5]; // 400ms（既定の窓 ±200ms）
            let at = 7_123;
            add_click(&mut x, at, f, rate, AMP_M30);
            let o = find_click_onset(&x, rate, f, 5.0).unwrap_or_else(|e| panic!("{rate} {f}: {e:?}"));
            assert!((o.pos - at as f64).abs() < 0.2, "{rate} {f}: pos {}", o.pos);
            assert!((o.peak - AMP_M30).abs() < AMP_M30 * 0.05, "{rate} {f}: peak {}", o.peak);
            assert!((o.width_ms - 4.5).abs() < 0.3, "{rate} {f}: width {}", o.width_ms);
        }
    }
    // クリックの長さを変えても（1ms・20ms）、最初の標本を指す
    for &burst_ms in &[1.0, 20.0] {
        let mut x = vec![0f32; 19_200];
        let b = click_burst(3000.0, 48_000, burst_ms, CLICK_FADE_MS, AMP_M30);
        x[5_000..5_000 + b.len()].copy_from_slice(&b);
        let o = find_click_onset(&x, 48_000, 3000.0, burst_ms).unwrap();
        assert!((o.pos - 5_000.0).abs() < 0.2, "{burst_ms}: {}", o.pos);
    }
}

#[test]
fn onset_survives_noise_and_other_tones() {
    let rate = 48_000;
    // 雑音（約 -56 dBFS の実効値）と、どのデバイスの高さでもない 2kHz・4kHz がずっと鳴っている（曲のつもり）
    let mut x = lcg_noise(19_200, 0.003, 7);
    add_sine(&mut x, 2000.0, rate, 0.01);
    add_sine(&mut x, 4000.0, rate, 0.01);
    // 3台のクリックが同じ入力に入る（仮想ケーブルを録るとき）。ずれは 0.2ms・-0.2ms
    add_click(&mut x, 9_000, 1000.0, rate, AMP_M30);
    add_click(&mut x, 9_010, 3000.0, rate, AMP_M30);
    add_click(&mut x, 8_990, 5000.0, rate, AMP_M30);
    // 重なったクリックの縁が少し漏れて偏る（下の overlapping_clicks_bias_is_small）。3 標本（48kHz で約 0.06ms）まで
    for (f, at) in [(1000.0, 9_000.0), (3000.0, 9_010.0), (5000.0, 8_990.0)] {
        let o = find_click_onset(&x, rate, f, 5.0).unwrap_or_else(|e| panic!("{f}: {e:?}"));
        assert!((o.pos - at).abs() < 3.0, "{f}: pos {} want {at}", o.pos);
        assert!(o.snr_db > ONSET_MIN_SNR_DB, "{f}: snr {}", o.snr_db);
    }
    // 雑音だけなら、ずれは 0.5 標本（約 10µs）未満
    for seed in 1..8 {
        let mut y = lcg_noise(19_200, 0.003, seed);
        add_click(&mut y, 9_000, 1000.0, rate, AMP_M30);
        let o = find_click_onset(&y, rate, 1000.0, 5.0).unwrap();
        assert!((o.pos - 9_000.0).abs() < 0.5, "seed {seed}: {}", o.pos);
    }
}

/// ほかのデバイスのクリックが前後 1.25ms 以内に重なる（録音デバイスに複数の出力が入るとき）。偏りの最大は約 0.06ms
/// （TONE_WINDOW_MS の決め方。1ms の素の窓だと約 0.15ms だった）
#[test]
fn overlapping_clicks_bias_is_small() {
    for &rate in &[48_000u32, 44_100] {
        for fi in 0..3usize {
            let freqs = [1000.0, 3000.0, 5000.0];
            let mut worst = 0f64;
            for off in (-60i64..=60).step_by(3) {
                let mut x = vec![0f32; 19_200];
                add_click(&mut x, 9_000, freqs[fi], rate, AMP_M30);
                for (gi, &g) in freqs.iter().enumerate().filter(|(gi, _)| *gi != fi) {
                    let at = 9_000 + if gi < fi { off } else { -off };
                    add_click(&mut x, at as usize, g, rate, AMP_M30);
                }
                let o = find_click_onset(&x, rate, freqs[fi], 5.0).unwrap_or_else(|e| panic!("{rate} {} {off}: {e:?}", freqs[fi]));
                worst = worst.max((o.pos - 9_000.0).abs() / rate as f64 * 1e6);
            }
            assert!(worst < 70.0, "{rate} {}: 最大 {worst:.1} µs", freqs[fi]);
        }
    }
}

#[test]
fn onset_refuses_what_is_not_the_click() {
    let rate = 48_000;
    let n = 19_200;
    // 無音・雑音だけ
    assert_eq!(find_click_onset(&vec![0f32; n], rate, 1000.0, 5.0), Err(HitMiss::NoSignal));
    assert!(find_click_onset(&lcg_noise(n, 0.003, 1), rate, 1000.0, 5.0).is_err());
    // ほかの高さのクリックだけ（1kHz を探すと 3kHz・5kHz は見えない）
    let mut other = vec![0f32; n];
    add_click(&mut other, 9_000, 3000.0, rate, AMP_M30);
    add_click(&mut other, 9_000, 5000.0, rate, AMP_M30);
    assert!(find_click_onset(&other, rate, 1000.0, 5.0).is_err(), "{:?}", find_click_onset(&other, rate, 1000.0, 5.0));
    // 同じ高さがずっと鳴っている（クリックが埋もれる）→ 小さい
    let mut steady = vec![0f32; n];
    add_sine(&mut steady, 1000.0, rate, 0.01);
    add_click(&mut steady, 9_000, 1000.0, rate, AMP_M30);
    assert_eq!(find_click_onset(&steady, rate, 1000.0, 5.0), Err(HitMiss::Weak));
    // 窓の端で切れている（始まりの前から・終わりの途中まで）
    let mut head_cut = vec![0f32; n];
    let b = click_burst(1000.0, rate, 5.0, CLICK_FADE_MS, AMP_M30);
    head_cut[..120].copy_from_slice(&b[120..]);
    assert_eq!(find_click_onset(&head_cut, rate, 1000.0, 5.0), Err(HitMiss::Truncated));
    let mut tail_cut = vec![0f32; n];
    tail_cut[n - 100..].copy_from_slice(&b[..100]);
    assert_eq!(find_click_onset(&tail_cut, rate, 1000.0, 5.0), Err(HitMiss::Truncated));
    // 長い音（50ms）→ 長さが違う
    let mut long = vec![0f32; n];
    let lb = click_burst(1000.0, rate, 50.0, CLICK_FADE_MS, AMP_M30);
    long[5_000..5_000 + lb.len()].copy_from_slice(&lb);
    assert_eq!(find_click_onset(&long, rate, 1000.0, 5.0), Err(HitMiss::WrongShape));
    // 同じ高さのクリックが2つ（100ms 離れて）→ 紛らわしい
    let mut two = vec![0f32; n];
    add_click(&mut two, 5_000, 1000.0, rate, AMP_M30);
    add_click(&mut two, 9_800, 1000.0, rate, AMP_M30 * 0.8);
    assert_eq!(find_click_onset(&two, rate, 1000.0, 5.0), Err(HitMiss::Ambiguous));
    // 一瞬だけ全部の高さが鳴る音（5ms の雑音のかたまり＝物音）→ クリックとしない
    let mut thump = lcg_noise(n, 0.0003, 3);
    for (i, v) in lcg_noise(240, 0.3, 4).into_iter().enumerate() {
        thump[9_000 + i] += v;
    }
    for f in [1000.0, 3000.0, 5000.0] {
        assert!(find_click_onset(&thump, rate, f, 5.0).is_err(), "{f}: {:?}", find_click_onset(&thump, rate, f, 5.0));
    }
}

/// 10ms ごとの、すき間の無いパケット
fn even_packets(n: usize, frames: u32, rate: u32, t0: u64) -> Vec<CapPacket> {
    (0..n)
        .map(|i| {
            let pos = i as u64 * frames as u64;
            CapPacket {
                pos,
                stored_before: pos,
                frames,
                capture_ns: t0 + (pos as f64 * 1e9 / rate as f64).round() as u64,
                callback_ns: 0,
                qpc_ns: 0,
            }
        })
        .collect()
}

#[test]
fn window_positions_cover_and_detect_gaps() {
    let (rate, t0, tol) = (48_000, 1_000_000_000u64, CAPTURE_GAP_TOL_NS);
    let p = even_packets(100, 480, rate, t0);
    let ms = |x: u64| x * 1_000_000;
    assert_eq!(window_positions(&p, rate, t0 + ms(105), t0 + ms(205), tol), Ok((5_040, 4_800, 5_040)));
    // まだ録れていない・始まる前
    assert_eq!(window_positions(&p, rate, t0 + ms(990), t0 + ms(1010), tol), Err(WindowMiss::NotYet));
    assert_eq!(window_positions(&[], rate, t0, t0 + 1, tol), Err(WindowMiss::NotYet));
    assert_eq!(window_positions(&p, rate, t0 - ms(50), t0 + ms(50), tol), Err(WindowMiss::NotCaptured));
    // パケットを1つ取りこぼした（通し番号が飛ぶ）→ 途切れ
    let mut dropped = p.clone();
    dropped.remove(15);
    assert_eq!(window_positions(&dropped, rate, t0 + ms(100), t0 + ms(200), tol), Err(WindowMiss::Gap));
    // その手前・後ろだけなら使える
    assert!(window_positions(&dropped, rate, t0 + ms(20), t0 + ms(120), tol).is_ok());
    // 時刻が 2ms 飛んだ → 途切れ。0.3ms の揺れはつながっている
    let mut jumped = p.clone();
    for q in jumped.iter_mut().skip(15) {
        q.capture_ns += 2_000_000;
    }
    assert_eq!(window_positions(&jumped, rate, t0 + ms(100), t0 + ms(200), tol), Err(WindowMiss::Gap));
    let mut jitter = p.clone();
    jitter[15].capture_ns += 300_000;
    assert!(window_positions(&jitter, rate, t0 + ms(100), t0 + ms(200), tol).is_ok());
    // 通し番号 → 時刻（そのフレームを含むパケットから）
    assert_eq!(pos_to_ns(&p, 480.0 * 3.0 + 240.0, rate), Some(t0 as f64 + 35_000_000.0));
    assert_eq!(pos_to_ns(&p, 1e9, rate), None);
    assert_eq!(pos_to_ns(&p, -1.0, rate), None);
}

/// コールバックの役（CaptureFeed）→ 輪 → 手元（CaptureTrack）→ 切り出し → 立ち上がりの QPC 時刻、を通しで
#[test]
fn feed_and_track_give_click_time() {
    for &rate in &[48_000u32, 44_100] {
        let frames = (rate / 100) as usize;
        let ch = 2;
        let (sp, mut sc) = rtrb::RingBuffer::<f32>::new(rate as usize * 4);
        let (pp, mut pc) = rtrb::RingBuffer::<CapPacket>::new(4096);
        let dropped = Arc::new(AtomicU64::new(0));
        let mut feed = CaptureFeed { samples: sp, packets: pp, pos: 0, stored: 0, dropped: dropped.clone() };
        let t0 = 5_000_000_000u64;
        // 3秒ぶんのステレオ。1.2345 秒の所に 1kHz のクリック（左右同じ）と、右だけに雑音
        let total = rate as usize * 3;
        let k0 = (1.2345 * rate as f64).round() as usize;
        let mut mono = vec![0f32; total];
        add_click(&mut mono, k0, 1000.0, rate, AMP_M30);
        let noise = lcg_noise(total, 0.001, 9);
        let mut track = CaptureTrack::new(rate);
        for (i, chunk) in mono.chunks(frames).enumerate() {
            let pos = (i * frames) as u64;
            // WASAPI と同じく 100ns 単位に丸めた時刻
            let cap = (t0 + (pos as f64 * 1e9 / rate as f64) as u64) / 100 * 100;
            let inter: Vec<f32> = chunk.iter().enumerate().flat_map(|(j, &v)| [v, v + noise[i * frames + j]]).collect();
            let m = inter.chunks_exact(ch).map(|f| f.iter().sum::<f32>() / ch as f32);
            feed.push(chunk.len(), m, cap, cap + 10_000_000, cap + 10_050_000);
            // 取り出す側（ときどき）
            if i % 7 == 0 {
                while let Ok(p) = pc.pop() {
                    track.add_packet(p);
                }
                let n = sc.slots();
                let c = sc.read_chunk(n).unwrap();
                let (a, b) = c.as_slices();
                track.add_samples(a);
                track.add_samples(b);
                c.commit_all();
                track.trim_before(t0 + 1_000_000_000);
            }
        }
        while let Ok(p) = pc.pop() {
            track.add_packet(p);
        }
        let n = sc.slots();
        let c = sc.read_chunk(n).unwrap();
        let (a, b) = c.as_slices();
        track.add_samples(a);
        track.add_samples(b);
        c.commit_all();
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        assert_eq!(track.breaks, 0);
        let truth = t0 as f64 + k0 as f64 * 1e9 / rate as f64;
        let target = truth as u64 + 3_000_000; // 狙いより 3ms 早く鳴った
        let (win, start) = track.window(target - 200_000_000, target + 200_000_000).unwrap();
        let o = find_click_onset(&win, rate, 1000.0, 5.0).unwrap();
        let ns = track.pos_to_ns(start as f64 + o.pos).unwrap();
        assert!((ns - truth).abs() < 5_000.0, "{rate}: {ns} vs {truth}（{} µs）", (ns - truth) / 1e3);
        assert!(((ns - target as f64) / 1e6 + 3.0).abs() < 0.01);
        // 古い所を捨てる（最後のパケットだけ残る）
        track.trim_before(u64::MAX);
        assert_eq!(track.packets.len(), 1);
        assert!(track.samples.len() <= frames, "{}", track.samples.len());
    }
}

#[test]
fn feed_drops_whole_packets_when_ring_is_full_and_track_sees_the_break() {
    let (sp, mut sc) = rtrb::RingBuffer::<f32>::new(1_000);
    let (pp, mut pc) = rtrb::RingBuffer::<CapPacket>::new(16);
    let dropped = Arc::new(AtomicU64::new(0));
    let mut feed = CaptureFeed { samples: sp, packets: pp, pos: 0, stored: 0, dropped: dropped.clone() };
    let mut track = CaptureTrack::new(48_000);
    let push = |feed: &mut CaptureFeed, k: u64| feed.push(480, std::iter::repeat(0.0), 1_000_000_000 + k * 10_000_000, 0, 0);
    push(&mut feed, 0);
    push(&mut feed, 1);
    push(&mut feed, 2); // 空きは 40 → 捨てる
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
    while let Ok(p) = pc.pop() {
        track.add_packet(p);
    }
    let c = sc.read_chunk(sc.slots()).unwrap();
    assert_eq!(c.len(), 960);
    c.commit_all();
    push(&mut feed, 3);
    let p = pc.pop().unwrap();
    assert_eq!((p.pos, p.stored_before), (1_440, 960));
    assert_eq!(track.add_packet(p), PacketLink::Broken(10_000_000.0));
    assert_eq!(track.breaks, 1);
}

fn click_event(dev: u8, click: u32, status: ClickStatus, target_ns: u64, aimed_off: i64) -> ClickEvent {
    ClickEvent {
        dev,
        open_no: if dev == 2 { click + 1 } else { 1 },
        click,
        status,
        target_ns,
        start_frame: 0,
        aimed_ns: target_ns as i64 + aimed_off,
        fit_minus_raw_ns: 0.0,
        fit_points: 10,
        l_ns: 0,
        used_fit: true,
        decided_ns: 0,
    }
}

#[test]
fn click_rows_give_differences_and_mark_unplayed() {
    let t = [10_000_000_000u64, 12_000_000_000];
    let sources = [CapSource::Loopback(1), CapSource::Loopback(2), CapSource::Input(1)];
    let events = [
        click_event(1, 0, ClickStatus::Ok, t[0], 5_000),
        click_event(1, 1, ClickStatus::Ok, t[1], -5_000),
        click_event(2, 0, ClickStatus::Ok, t[0], 0),
        click_event(2, 1, ClickStatus::Late, t[1], 0),
    ];
    let on = |ns: f64| -> Result<(f64, Onset), HitMiss> { Ok((ns, Onset { pos: 0.0, peak: AMP_M30, snr_db: 60.0, width_ms: 4.5 })) };
    let us = |x: f64| x * 1e3;
    let hits = [
        Hit { stream: 0, dev: 1, click: 0, res: on(t[0] as f64 + us(100.0)) },
        Hit { stream: 0, dev: 1, click: 1, res: on(t[1] as f64 + us(200.0)) },
        Hit { stream: 1, dev: 2, click: 0, res: on(t[0] as f64 + us(600.0)) },
        Hit { stream: 1, dev: 2, click: 1, res: on(t[1] as f64) }, // 鳴らしていないのに見つかった
        Hit { stream: 2, dev: 1, click: 0, res: on(t[0] as f64 + us(20_000.0)) },
        Hit { stream: 2, dev: 2, click: 0, res: on(t[0] as f64 + us(20_500.0)) },
        Hit { stream: 2, dev: 1, click: 1, res: Err(HitMiss::Weak) },
        Hit { stream: 2, dev: 2, click: 1, res: Err(HitMiss::NoSignal) },
    ];
    let rows = click_rows(&hits, &sources, &t, &events);
    assert_eq!(rows.len(), 8);
    let get = |s: usize, d: u8, c: usize| *rows.iter().find(|r| (r.stream, r.dev, r.click) == (s, d, c)).unwrap();
    let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1.0);
    // lb1 は基準（基準との差は無い）。狙いとの差・選んだフレームの丸め
    let r = get(0, 1, 0);
    assert!(close(r.minus_target_ns(), us(100.0)) && r.minus_base_ns.is_none() && close(r.aimed_minus_target_ns, 5_000.0));
    assert_eq!(r.status(), "ok");
    // lb2：基準（lb1）との差 0.5ms。途中参加なら開き直しの回は click＋1
    let r = get(1, 2, 0);
    assert!(close(r.minus_base_ns, us(500.0)), "{r:?}");
    assert_eq!(r.open_no, 1);
    let r = get(1, 2, 1);
    assert_eq!((r.status(), r.open_no, r.minus_target_ns()), ("not_played", 2, None));
    // 録音デバイス：その中の 1kHz との差と、lb1 との差
    let r = get(2, 2, 0);
    assert!(close(r.minus_base_ns, us(500.0)) && close(r.minus_lb1_ns, us(20_400.0)), "{r:?}");
    let r = get(2, 1, 0);
    assert!(r.minus_base_ns.is_none() && close(r.minus_lb1_ns, us(19_900.0)), "{r:?}");
    // 2回目：1kHz は小さくて取れず、2台目はもともと鳴らしていない（遅れて鳴らさなかった回は、見つからなくても not_played）
    assert_eq!(get(2, 1, 1).status(), "weak");
    assert_eq!(get(2, 2, 1).status(), "not_played");
    // 集計
    let st = section_stats(rows.iter().filter(|r| r.stream == 1));
    assert_eq!((st.vs_target.len(), st.vs_base.len(), st.not_played, st.found_unplayed), (1, 1, 1, 1));
    let st = section_stats(rows.iter().filter(|r| r.stream == 2 && r.dev == 1));
    assert_eq!(st.misses.get(&HitMiss::Weak), Some(&1));
    assert_eq!(st.vs_lb1.len(), 1);
}

#[test]
fn diff_stats_and_capture_settings() {
    assert_eq!(diff_stats(&[]), None);
    let d = diff_stats(&[1.0, 2.0, 3.0, 4.0]).unwrap();
    assert_eq!((d.n, d.mean, d.min, d.max), (4, 2.5, 1.0, 4.0));
    assert!((d.std - 1.25f64.sqrt()).abs() < 1e-12);
    // 既定の大きさは範囲の中。ループバックは小さめ
    assert!(level_amp("PL_PROBE_CLICK_DB", CLICK_DB_LOOPBACK).is_ok());
    assert!(level_amp("PL_PROBE_CLICK_DB", CLICK_DB_RECORD).is_ok());
    assert!(CLICK_DB_LOOPBACK < CLICK_DB_RECORD);
    assert_eq!(ClickMode::Loopback.default_db(), -30.0);
    assert_eq!(ClickMode::Record.default_db(), -12.0);
    // 録音デバイスの選び方（指定の仕方は PL_PROBE_DEVICES と同じ。既定が無いとき・空のときの知らせは「録音」）
    let inputs = vec![dev("{x}", "マイク (USB)", false), dev("{y}", "仮想ケーブルの出口 (Virtual Audio)", false)];
    assert_eq!(select_from(&inputs, "virtual", "録音", "PL_PROBE_CAPTURE").unwrap(), vec![1]);
    assert_eq!(select_from(&inputs, "#0|{y}", "録音", "PL_PROBE_CAPTURE").unwrap(), vec![0, 1]);
    assert!(select_from(&inputs, "default", "録音", "PL_PROBE_CAPTURE").unwrap_err().contains("録音"));
    assert!(select_from(&inputs, "|", "録音", "PL_PROBE_CAPTURE").unwrap_err().contains("PL_PROBE_CAPTURE"));
    assert!(device_list_text_of(&[], "録音").contains("録音デバイスが見つからない"));
    assert_eq!(CapSource::Loopback(2).label(), "lb2");
    assert_eq!(CapSource::Input(1).label(), "cap1");
}
