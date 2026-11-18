//! 同時再生の時計と、出力どうしのずれの直し方（v3.8.0。仕様「同時再生と配信モード」13.1）。
//!
//! ここには、音を鳴らさずに確かめられる部分だけを置く（時計・直線の当てはめ・PI 制御と不感帯・開始時刻の門・
//! 基準の選び方・つなぎ直しの間隔）。デバイスを開く部分は `output.rs`、出力ごとの流し込みは `lane.rs`、
//! 組み立ては `thread.rs`。
//!
//! 時刻はどれも Windows の共通の時計（QPC）を ns にしたもの。cpal の WASAPI の時刻も QPC を 100ns 単位にしたもの
//! （13.1 の2）なので、そのまま比べる。曲の位置は「曲の側の輪のレート」のフレーム（小数）で持つ。

use std::collections::VecDeque;

/// 仮の値（2026-11-15〜16 の実測で決めた・まだ決めきれない値）。差し替えやすいよう、ここ1か所にまとめる。
/// どれも仕様 13.1・13.3 の「暫定」。作者の実機の 30 分・1 時間の計測で決め直す
pub mod tuning {
    /// 位置の項の不感帯（ms）。開くたびに 0〜1ms の見えない差が乗る（13.3 の途中参加20回）ので、これより細かく合わせない
    pub const DEADBAND_MS: f64 = 1.0;
    /// 倍率の上限（名目に対して ±0.1%）。rubato の `Async` を作るときの上限にも使う
    pub const MAX_RATIO_REL: f64 = 1.001;
    /// 遅らせ量の上限（ms。仕様 2.3）
    pub const DELAY_MAX_MS: u32 = 1000;
    /// 曲の側の輪：遅らせ量の上限＋先読み＋余裕（13.1 の5）
    pub const RING_DELAY_MS: u32 = DELAY_MAX_MS;
    pub const RING_LOOKAHEAD_MS: u32 = 500;
    pub const RING_MARGIN_MS: u32 = 500;
    /// 開いてからこの間のコールバックは当てはめに使わない（13.1 の3）
    pub const SKIP_AFTER_OPEN_MS: u64 = 200;
    /// 開始時刻の門：今からこれだけ先に開く（13.1 の6）
    pub const GATE_LEAD_MS: u64 = 80;
    /// 門を開いたあと音を上げる長さ（13.1 の6）
    pub const FADE_IN_MS: u32 = 10;
    /// 倍率を決め直す間隔（13.1 の4）
    pub const CONTROL_INTERVAL_MS: u64 = 100;
    /// ずれがこれを超えたら、倍率で追わずに入り直す（13.1 の4）
    pub const REENTRY_MS: f64 = 20.0;
    /// 位置の当てはめの窓（ms）。13.3 の計測道具の既定と同じ
    pub const FIT_WINDOW_MS: u64 = 3000;
    /// 速さの項に使う長い当てはめの窓（ms）と、そこへ点を足す間隔
    pub const SPEED_WINDOW_MS: u64 = 30_000;
    pub const SPEED_POINT_EVERY_MS: u64 = 100;
    /// 基準を決めるまでに見る長さ（13.1 の1「開いて約2秒」）
    pub const REFERENCE_PICK_AFTER_MS: u64 = 2000;
    /// 揺れの差がこれ未満なら、枠の番号が小さいほうを基準に（13.1 の1）
    pub const REFERENCE_TIE_MS: f64 = 0.2;
    /// 基準の揺れがこれを超え続けたら替える（13.1 の1。暫定 2ms）
    pub const REFERENCE_JITTER_MAX_MS: f64 = 2.0;
    /// 「超え続けた」とみなす長さ（ms）
    pub const REFERENCE_JITTER_HOLD_MS: u64 = 5000;
    /// cpal の `playback` から GetStreamLatency（L）を引くか（13.1 の3。暫定は「引く」。作者の実機の機器では L が 0）
    pub const SUBTRACT_STREAM_LATENCY: bool = true;
    /// 音声スレッドがこれ以上空いたら（スリープ明けなど）、時計を止めて付け直す（13.1 の1）
    pub const STALL_RESET_MS: u64 = 1000;
    /// 自前の QPC と `stream.now()` の差がこれ以上なら、同期を切って知らせる（13.1 の2）
    pub const QPC_STREAM_TOLERANCE_MS: f64 = 1.0;
    /// 遅らせ量を変えてから反映するまで（つまみを止めて約150ms。13.1 の5）
    pub const DELAY_APPLY_AFTER_MS: u64 = 150;
    /// PI 制御の係数。位置の項（不感帯を引いたずれ ms）に掛けて、倍率の相対の変化にする。
    /// 比例：5ms のずれで上限の 0.1%（時定数 約5秒）。積分：比例の 1/10 の強さ
    pub const KP_PER_MS: f64 = 2.0e-4;
    pub const KI_PER_MS_S: f64 = 2.0e-5;
    /// つなぎ直しの間隔（13.5 の12）：知らせの直後に試し、だめなら 0.5・1・2 秒あけて3回、
    /// そのあとデバイスが一覧にある間は 5 秒おき（暫定）
    pub const RETRY_STEPS_MS: [u64; 3] = [500, 1000, 2000];
    pub const RETRY_EVERY_MS: u64 = 5000;
    /// 開く役が実際に開き始めてから、これだけ待っても結果が来なければ「開けません（その他）」と出す（頼みは取り消さず、遅れて届いた結果も採る。検査の指摘から。暫定）
    pub const OPEN_TIMEOUT_MS: u64 = 10_000;
    /// デバイスの変化の知らせをまとめる間（13.5 の12）
    pub const DEVICE_NOTICE_DEBOUNCE_MS: u64 = 250;
    /// メーターを画面へ送る間隔（約 30 回／秒。2.7）
    pub const METER_INTERVAL_MS: u64 = 33;
}

const NS_PER_MS: f64 = 1_000_000.0;

/// QPC の値を ns にする。cpal（`stream.rs` の `now`）と同じく、先に 100ns 単位へ丸める
pub fn qpc_to_ns(counter: i64, freq: i64) -> u64 {
    if counter <= 0 || freq <= 0 {
        return 0;
    }
    let units_100ns = counter as u128 * 10_000_000 / freq as u128;
    (units_100ns * 100).min(u64::MAX as u128) as u64
}

/// 今の QPC（ns）
#[cfg(windows)]
pub fn qpc_now_ns() -> u64 {
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
pub fn qpc_now_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

pub fn ms_to_ns(ms: u64) -> u64 {
    ms.saturating_mul(1_000_000)
}

/// 直線 y = intercept + slope · x
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub slope: f64,
    pub intercept: f64,
}

impl Line {
    pub fn at(&self, x: f64) -> f64 {
        self.intercept + self.slope * x
    }
    pub fn x_for(&self, y: f64) -> f64 {
        (y - self.intercept) / self.slope
    }
    pub fn through(x: f64, y: f64, slope: f64) -> Self {
        Self { slope, intercept: y - slope * x }
    }
}

/// 最小二乗で直線を当てる（平均を引いてから計算する）。2点未満・x が全部同じなら None
pub fn fit_line<I>(pts: I) -> Option<Line>
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
    if sxx <= 0.0 || !sxx.is_finite() || !sxy.is_finite() {
        return None;
    }
    let slope = sxy / sxx;
    if slope.is_nan() || slope <= 0.0 {
        return None;
    }
    Some(Line { slope, intercept: my - slope * mx })
}

/// 当てはめの残り（y − 直線）の標準偏差（y の単位）
pub fn residual_std<I>(pts: I, line: &Line) -> f64
where
    I: Iterator<Item = (f64, f64)>,
{
    let (mut n, mut s, mut ss) = (0usize, 0f64, 0f64);
    for (x, y) in pts {
        let r = y - line.at(x);
        n += 1;
        s += r;
        ss += r * r;
    }
    if n == 0 {
        return 0.0;
    }
    let mean = s / n as f64;
    (ss / n as f64 - mean * mean).max(0.0).sqrt()
}

/// 直近の点だけを持つ当てはめの窓（x：出力のフレーム、y：鳴る時刻 ns の相対、t：記録した時刻 ns）。
/// 音声スレッド（コールバックではない）で使う
#[derive(Debug, Clone)]
pub struct FitWindow {
    pts: VecDeque<(f64, f64, u64)>,
    window_ns: u64,
}

impl FitWindow {
    pub fn new(window_ms: u64) -> Self {
        Self { pts: VecDeque::new(), window_ns: ms_to_ns(window_ms) }
    }
    pub fn push(&mut self, x: f64, y: f64, t_ns: u64) {
        self.pts.push_back((x, y, t_ns));
        while let Some(&(_, _, t0)) = self.pts.front() {
            if t_ns.saturating_sub(t0) > self.window_ns && self.pts.len() > 2 {
                self.pts.pop_front();
            } else {
                break;
            }
        }
    }
    pub fn len(&self) -> usize {
        self.pts.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pts.is_empty()
    }
    pub fn clear(&mut self) {
        self.pts.clear();
    }
    pub fn fit(&self) -> Option<Line> {
        fit_line(self.pts.iter().map(|&(x, y, _)| (x, y)))
    }
    /// 当てはめの残りの揺れ（ms）
    pub fn jitter_ms(&self) -> Option<f64> {
        let line = self.fit()?;
        Some(residual_std(self.pts.iter().map(|&(x, y, _)| (x, y)), &line) / NS_PER_MS)
    }
    /// 窓の中のいちばん古い点の記録時刻
    pub fn first_t(&self) -> Option<u64> {
        self.pts.front().map(|p| p.2)
    }
    pub fn last_t(&self) -> Option<u64> {
        self.pts.back().map(|p| p.2)
    }
}

/// 出力1本の「フレーム → 鳴る時刻」の見積もり。コールバックの記録（そのコールバックの最初のフレームの番号と、
/// それが鳴る時刻）から作る。開いて `SKIP_AFTER_OPEN_MS` までと、頭打ち（cpal が前に戻さないよう止めた回）は使わない
#[derive(Debug, Clone)]
pub struct StreamTiming {
    nominal_rate: u32,
    /// 最初のコールバックの callback / playback（ns）と L
    first_cb: Option<u64>,
    origin_ns: u64,
    latency_ns: u64,
    prev_pb: Option<u64>,
    fit: FitWindow,
    speed: FitWindow,
    last_speed_point: Option<u64>,
    /// 頭打ちの回数（記録のため）
    pub stalls: u64,
}

impl StreamTiming {
    pub fn new(nominal_rate: u32) -> Self {
        Self {
            nominal_rate: nominal_rate.max(1),
            first_cb: None,
            origin_ns: 0,
            latency_ns: 0,
            prev_pb: None,
            fit: FitWindow::new(tuning::FIT_WINDOW_MS),
            speed: FitWindow::new(tuning::SPEED_WINDOW_MS),
            last_speed_point: None,
            stalls: 0,
        }
    }

    /// コールバック1回ぶんの記録を足す
    pub fn add(&mut self, frames_before: u64, callback_ns: u64, playback_ns: u64) {
        if self.first_cb.is_none() {
            self.first_cb = Some(callback_ns);
            self.latency_ns = playback_ns.saturating_sub(callback_ns);
            self.origin_ns = playback_ns;
        }
        let stalled = matches!(self.prev_pb, Some(p) if playback_ns <= p);
        self.prev_pb = Some(playback_ns);
        if stalled {
            self.stalls += 1;
            return;
        }
        let first = self.first_cb.unwrap_or(callback_ns);
        if callback_ns < first + ms_to_ns(tuning::SKIP_AFTER_OPEN_MS) {
            return;
        }
        let y = playback_ns as f64 - self.origin_ns as f64;
        self.fit.push(frames_before as f64, y, callback_ns);
        let due = self
            .last_speed_point
            .map_or(true, |t| callback_ns.saturating_sub(t) >= ms_to_ns(tuning::SPEED_POINT_EVERY_MS));
        if due {
            self.speed.push(frames_before as f64, y, callback_ns);
            self.last_speed_point = Some(callback_ns);
        }
    }

    /// 門を開けるだけの記録があるか（開いて約200ms＋数回）
    pub fn ready(&self) -> bool {
        self.fit.len() >= 4 && self.fit.fit().is_some()
    }

    /// 当てはめの記録が続いている長さ（ms）
    pub fn observed_ms(&self) -> u64 {
        match (self.first_cb, self.fit.last_t()) {
            (Some(a), Some(b)) => b.saturating_sub(a) / 1_000_000,
            _ => 0,
        }
    }

    pub fn jitter_ms(&self) -> Option<f64> {
        self.fit.jitter_ms()
    }

    /// L を引くかどうかを入れた「鳴る時刻」の補正（ns）
    fn latency_offset(&self) -> f64 {
        if tuning::SUBTRACT_STREAM_LATENCY {
            self.latency_ns as f64
        } else {
            0.0
        }
    }

    /// フレーム `k` が鳴る時刻（QPC ns）
    pub fn time_of_frame(&self, k: f64) -> Option<f64> {
        let line = self.fit.fit()?;
        Some(line.at(k) + self.origin_ns as f64 - self.latency_offset())
    }

    /// 時刻 `t_ns` に鳴るフレーム（小数）
    pub fn frame_at(&self, t_ns: f64) -> Option<f64> {
        let line = self.fit.fit()?;
        Some(line.x_for(t_ns - self.origin_ns as f64 + self.latency_offset()))
    }

    /// 位置の当てはめの傾き（1フレームの ns）。遅れを測る機能の記録用（v3.8.0 段3。名目との差で、狙いの見積もりの確かさを見る）
    pub fn fit_slope(&self) -> Option<f64> {
        self.fit.fit().map(|l| l.slope)
    }

    /// 1フレームが鳴るのにかかる ns。長い当てはめがあればそれ、無ければ位置の当てはめ、それも無ければ名目
    pub fn ns_per_frame(&self) -> f64 {
        let nominal = 1e9 / self.nominal_rate as f64;
        let pick = |l: Option<Line>| l.map(|l| l.slope).filter(|s| (*s / nominal - 1.0).abs() < 0.01);
        if self.speed.len() >= 10 {
            if let Some(s) = pick(self.speed.fit()) {
                return s;
            }
        }
        pick(self.fit.fit()).unwrap_or(nominal)
    }

    /// 時刻の飛び（スリープ明けなど）のあとに、当てはめをやり直す
    pub fn reset(&mut self) {
        self.first_cb = None;
        self.prev_pb = None;
        self.fit.clear();
        self.speed.clear();
        self.last_speed_point = None;
    }
}

/// 仮想の再生時計（13.1 の1）。出力が無くても進む。位置は曲の側の輪のフレーム（小数）。
/// 止めている間・開始時刻より前は、その位置のまま
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    anchor_ns: u64,
    anchor_pos: f64,
    /// 曲のフレーム / ns
    speed: f64,
    running: bool,
}

impl Clock {
    pub fn new(rate: u32, pos: f64) -> Self {
        Self { anchor_ns: 0, anchor_pos: pos, speed: Self::nominal_speed(rate), running: false }
    }
    pub fn nominal_speed(rate: u32) -> f64 {
        rate.max(1) as f64 / 1e9
    }
    pub fn running(&self) -> bool {
        self.running
    }
    /// 走っていて、開始時刻を過ぎた
    pub fn started(&self, now_ns: u64) -> bool {
        self.running && now_ns >= self.anchor_ns
    }
    pub fn speed(&self) -> f64 {
        self.speed
    }
    /// 時刻 `t_ns` の位置
    pub fn pos_at(&self, t_ns: f64) -> f64 {
        if !self.running || t_ns <= self.anchor_ns as f64 {
            self.anchor_pos
        } else {
            self.anchor_pos + (t_ns - self.anchor_ns as f64) * self.speed
        }
    }
    /// `t_ns` から `pos` で進め始める（それより前は `pos` のまま）
    pub fn start_at(&mut self, t_ns: u64, pos: f64) {
        self.anchor_ns = t_ns;
        self.anchor_pos = pos;
        self.running = true;
    }
    /// 止める（位置は `now` のまま）
    pub fn stop(&mut self, now_ns: u64) {
        self.anchor_pos = self.pos_at(now_ns as f64);
        self.anchor_ns = now_ns;
        self.running = false;
    }
    /// 止めたまま位置を変える（シーク・曲を開く）
    pub fn set_stopped(&mut self, pos: f64) {
        self.anchor_pos = pos;
        self.running = false;
    }
    /// 速さを替えて、今の位置から続ける（位置は跳ばない）
    pub fn set_speed(&mut self, now_ns: u64, speed: f64) {
        if speed.is_nan() || speed <= 0.0 || !speed.is_finite() {
            return;
        }
        if self.running && now_ns as f64 >= self.anchor_ns as f64 {
            self.anchor_pos = self.pos_at(now_ns as f64);
            self.anchor_ns = now_ns;
        }
        self.speed = speed;
    }
    /// 基準の出力に合わせる：`now` の位置を `pos`、速さを `speed` にする（走っている間・開始時刻を過ぎたときだけ）
    pub fn follow(&mut self, now_ns: u64, pos: f64, speed: f64) {
        if !self.running || now_ns < self.anchor_ns || !pos.is_finite() || speed.is_nan() || speed <= 0.0 {
            return;
        }
        self.anchor_ns = now_ns;
        self.anchor_pos = pos;
        self.speed = speed;
    }
}

/// 不感帯：|e| が帯の中なら 0、外なら帯の分を引いた値
pub fn deadband(e: f64, band: f64) -> f64 {
    if !e.is_finite() || e.abs() <= band {
        0.0
    } else {
        e - band * e.signum()
    }
}

/// 位置の項の PI 制御（13.1 の4）。戻り値は倍率（出力 / 入力）に掛ける相対の補正。
/// 正のずれ（先へ行きすぎ）なら正の補正（出力を増やして曲の進みを遅くする）
#[derive(Debug, Clone, Default)]
pub struct Pi {
    integral: f64,
}

impl Pi {
    pub fn reset(&mut self) {
        self.integral = 0.0;
    }
    pub fn update(&mut self, error_ms: f64, dt_s: f64) -> f64 {
        let cap = tuning::MAX_RATIO_REL - 1.0;
        let e = deadband(error_ms, tuning::DEADBAND_MS);
        if e != 0.0 && dt_s.is_finite() && dt_s > 0.0 {
            self.integral += e * dt_s.min(1.0);
            // 積分だけで上限を超えないように抑える（巻き上がり防止）
            let lim = cap / tuning::KI_PER_MS_S;
            self.integral = self.integral.clamp(-lim, lim);
        }
        (tuning::KP_PER_MS * e + tuning::KI_PER_MS_S * self.integral).clamp(-cap, cap)
    }
}

/// 倍率（出力フレーム / 入力フレーム）を、名目に対して上限の中へ収める
pub fn clamp_ratio(ratio: f64, nominal: f64) -> f64 {
    if !ratio.is_finite() || ratio <= 0.0 {
        return nominal;
    }
    ratio.clamp(nominal / tuning::MAX_RATIO_REL, nominal * tuning::MAX_RATIO_REL)
}

/// 追う側の出力の倍率を決める。
/// - 速さの項：時計の速さ（曲のフレーム / ns）× その出力の 1フレームの ns ＝ 1出力フレームで進むべき曲のフレーム
/// - 位置の項：`adj`（PI 制御の出力）
pub fn follower_ratio(clock_speed: f64, out_ns_per_frame: f64, adj: f64, nominal: f64) -> f64 {
    let in_per_out = clock_speed * out_ns_per_frame;
    let ff = if in_per_out > 0.0 && in_per_out.is_finite() { 1.0 / in_per_out } else { nominal };
    clamp_ratio(ff * (1.0 + adj), nominal)
}

/// 開始時刻の門（13.1 の6）。出力のフレーム `k_start` から、曲の位置 `song_start` を鳴らす
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub k_start: u64,
    pub t_start_ns: f64,
    pub song_start: f64,
}

/// `now` から `GATE_LEAD_MS` 先に門を開く。フレームは切り上げ（狙った時刻より前には鳴らさない）。
/// `delay_frames` はその出力の遅らせ量（曲のフレーム）
pub fn plan_gate(timing: &StreamTiming, clock: &Clock, now_ns: u64, delay_frames: f64) -> Option<Gate> {
    let t_target = (now_ns + ms_to_ns(tuning::GATE_LEAD_MS)) as f64;
    let k = timing.frame_at(t_target)?.ceil();
    if !k.is_finite() || k < 0.0 {
        return None;
    }
    let t_k = timing.time_of_frame(k)?;
    Some(Gate { k_start: k as u64, t_start_ns: t_k, song_start: clock.pos_at(t_k) - delay_frames })
}

/// 基準の選び方（13.1 の1）：揺れがいちばん小さいもの。差が `REFERENCE_TIE_MS` 未満なら枠の番号が小さいほう。
/// `cands` は（枠の番号, 揺れ ms）
pub fn pick_reference(cands: &[(usize, f64)]) -> Option<usize> {
    let min = cands.iter().map(|c| c.1).filter(|j| j.is_finite()).fold(f64::INFINITY, f64::min);
    if !min.is_finite() {
        return cands.iter().map(|c| c.0).min();
    }
    cands
        .iter()
        .filter(|c| c.1.is_finite() && c.1 - min < tuning::REFERENCE_TIE_MS)
        .map(|c| c.0)
        .min()
}

/// つなぎ直しの次の試しまでの間（ms）。`attempt` は失敗した回数（0 から）。
/// デバイスが一覧に無い間は試さない（知らせを待つ）
pub fn retry_delay_ms(attempt: u32, in_list: bool) -> Option<u64> {
    if !in_list {
        return None;
    }
    Some(tuning::RETRY_STEPS_MS.get(attempt as usize).copied().unwrap_or(tuning::RETRY_EVERY_MS))
}

/// 曲の終わり（13.1 の1・13.2）：デコードを読み切り、時計が「曲の長さ＋オンの出力の遅らせ量の最大」に届いた
pub fn track_ended(eof: bool, end_frame: i64, clock_pos: f64, max_delay_frames: f64) -> bool {
    eof && clock_pos >= end_frame as f64 + max_delay_frames.max(0.0)
}

/// 遅らせ量（ms）を曲のフレームに
pub fn delay_frames(delay_ms: u32, song_rate: u32) -> f64 {
    delay_ms.min(tuning::DELAY_MAX_MS) as f64 * song_rate as f64 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// 名目より ppm だけ速い（＋）・遅い（−）出力の記録を作る。1回のコールバックは 480 フレーム
    fn feed(t: &mut StreamTiming, start_ns: u64, ppm: f64, count: u64, jitter: impl Fn(u64) -> i64) {
        let ns_per_frame = 1e9 / (RATE as f64 * (1.0 + ppm * 1e-6));
        for i in 0..count {
            let frames = i * 480;
            let pb = start_ns as f64 + frames as f64 * ns_per_frame + jitter(i) as f64;
            t.add(frames, pb as u64, pb as u64);
        }
    }

    /// 時計の当てはめ：開いて 200ms は捨て、残りの直線から「フレーム → 時刻」と「時刻 → フレーム」が往復で一致する。
    /// 速さ（1フレームの ns）は名目に対する ppm の差を表す
    #[test]
    fn timing_fit_round_trips_and_skips_the_first_200ms() {
        let mut t = StreamTiming::new(RATE);
        assert!(!t.ready());
        // 最初の 200ms（20 回）は当てはめに入らない
        feed(&mut t, 1_000_000_000, 0.0, 20, |_| 0);
        assert!(!t.ready());
        let mut t = StreamTiming::new(RATE);
        feed(&mut t, 1_000_000_000, 20.0, 400, |_| 0);
        assert!(t.ready());
        let k = 100_000.0;
        let at = t.time_of_frame(k).unwrap();
        assert!((t.frame_at(at).unwrap() - k).abs() < 1e-3);
        let ns = t.ns_per_frame();
        let ppm = (1e9 / (ns * RATE as f64) - 1.0) * 1e6;
        assert!((ppm - 20.0).abs() < 0.5, "ppm {ppm}");
        assert!(t.observed_ms() >= 3000);
    }

    /// 頭打ち（playback が前の回より進んでいない）は当てはめから外し、数える
    #[test]
    fn timing_ignores_stalled_callbacks() {
        let mut t = StreamTiming::new(RATE);
        feed(&mut t, 0, 0.0, 100, |_| 0);
        let before = t.fit.len();
        // 同じ playback をもう一度（頭打ち）
        t.add(100 * 480, 990_000_000, 990_000_000);
        assert_eq!(t.stalls, 1);
        assert_eq!(t.fit.len(), before);
    }

    /// 揺れ（当てはめの残りの標準偏差）を ms で出す
    #[test]
    fn timing_jitter_is_reported_in_ms() {
        let mut t = StreamTiming::new(RATE);
        // ±0.5ms を交互に
        feed(&mut t, 0, 0.0, 300, |i| if i % 2 == 0 { 500_000 } else { -500_000 });
        let j = t.jitter_ms().unwrap();
        assert!((j - 0.5).abs() < 0.05, "jitter {j}");
    }

    /// 時計：止めている間・開始時刻より前は動かず、走らせると名目の速さで進む。止めると位置が残る。
    /// 速さを替えても、位置は跳ばない
    #[test]
    fn clock_runs_stops_and_keeps_position() {
        let mut c = Clock::new(RATE, 4800.0);
        assert_eq!(c.pos_at(5e9), 4800.0);
        c.start_at(1_000_000_000, 4800.0);
        assert_eq!(c.pos_at(0.5e9), 4800.0);
        assert!((c.pos_at(2e9) - (4800.0 + 48_000.0)).abs() < 1e-6);
        c.set_speed(2_000_000_000, Clock::nominal_speed(RATE) * 1.001);
        assert!((c.pos_at(2e9) - 52_800.0).abs() < 1e-6);
        assert!((c.pos_at(3e9) - (52_800.0 + 48_048.0)).abs() < 1e-3);
        c.stop(3_000_000_000);
        assert!(!c.running());
        assert!((c.pos_at(9e9) - 100_848.0).abs() < 1e-3);
        // 止めたまま位置を変える（シーク）
        c.set_stopped(96_000.0);
        assert_eq!(c.pos_at(1e12), 96_000.0);
        // 開始時刻より前の follow は効かない
        c.start_at(10_000_000_000, 96_000.0);
        c.follow(9_000_000_000, 1.0, 1.0);
        assert_eq!(c.pos_at(10e9), 96_000.0);
    }

    /// 不感帯：±1ms の中は 0。外は帯の分を引く（帯の端で跳ばない）。数でない値は 0
    #[test]
    fn deadband_is_one_ms_and_continuous() {
        assert_eq!(tuning::DEADBAND_MS, 1.0);
        assert_eq!(deadband(0.9, 1.0), 0.0);
        assert_eq!(deadband(-1.0, 1.0), 0.0);
        assert!((deadband(1.5, 1.0) - 0.5).abs() < 1e-12);
        assert!((deadband(-3.0, 1.0) + 2.0).abs() < 1e-12);
        assert_eq!(deadband(f64::NAN, 1.0), 0.0);
    }

    /// PI 制御：不感帯の中なら補正 0（積分も貯めない）。先へ行きすぎ（正のずれ）なら正の補正。
    /// 補正は上限（±0.1%）で頭打ち、積分も上限を超えて貯まらない
    #[test]
    fn pi_respects_deadband_sign_and_limits() {
        let mut pi = Pi::default();
        for _ in 0..100 {
            assert_eq!(pi.update(0.8, 0.1), 0.0);
        }
        assert_eq!(pi.integral, 0.0);
        let a = pi.update(3.0, 0.1);
        assert!(a > 0.0);
        let b = Pi::default().update(-3.0, 0.1);
        assert!(b < 0.0);
        let cap = tuning::MAX_RATIO_REL - 1.0;
        let mut pi = Pi::default();
        for _ in 0..10_000 {
            let v = pi.update(50.0, 0.1);
            assert!(v <= cap + 1e-15);
        }
        assert!((pi.update(50.0, 0.1) - cap).abs() < 1e-12);
        // 積分は上限の分しか貯まらないので、ずれが戻ればすぐ抜ける
        let mut steps = 0;
        while pi.update(-50.0, 0.1) > 0.0 {
            steps += 1;
            assert!(steps < 200, "integral wound up");
        }
    }

    /// 閉じた輪で確かめる：12ppm 遅い出力が、基準（名目）の時計を PI で追う。
    /// 5ms ずれた所から始めても、不感帯（±1ms）の中へ入り、そこに留まる。倍率は上限の中
    #[test]
    fn pi_loop_pulls_a_slow_device_into_the_deadband() {
        let nominal = 1.0; // 同じレート
        let clock_speed = Clock::nominal_speed(RATE);
        let out_ns_per_frame = 1e9 / (RATE as f64 * (1.0 - 12e-6));
        let mut pi = Pi::default();
        let mut err_ms = 5.0f64;
        let dt = 0.1;
        let mut max_late = 0.0f64;
        for step in 0..3000 {
            let adj = pi.update(err_ms, dt);
            let ratio = follower_ratio(clock_speed, out_ns_per_frame, adj, nominal);
            assert!(ratio >= nominal / tuning::MAX_RATIO_REL && ratio <= nominal * tuning::MAX_RATIO_REL);
            // 0.1 秒で、出力は out_frames だけ鳴らし、曲は out_frames / ratio 進む。時計は名目で進む
            let out_frames = dt * 1e9 / out_ns_per_frame;
            let song_adv = out_frames / ratio;
            let clock_adv = dt * 1e9 * clock_speed;
            err_ms += (song_adv - clock_adv) / RATE as f64 * 1000.0;
            if step > 1500 {
                max_late = max_late.max(err_ms.abs());
            }
        }
        assert!(max_late <= tuning::DEADBAND_MS + 0.05, "settled error {max_late} ms");
    }

    /// 速さの項：出力が名目より 12ppm 遅ければ、倍率は名目より少し小さく（曲を少し速く進める）。上限の外は切る
    #[test]
    fn follower_ratio_feed_forward_and_clamp() {
        let speed = Clock::nominal_speed(RATE);
        let slow = 1e9 / (RATE as f64 * (1.0 - 12e-6));
        let r = follower_ratio(speed, slow, 0.0, 1.0);
        assert!(r < 1.0 && r > 1.0 - 20e-6, "{r}");
        // 違うレート（44.1k の曲 → 48k の出力）の名目は 48000/44100
        let nominal = 48_000.0 / 44_100.0;
        let r = follower_ratio(Clock::nominal_speed(44_100), 1e9 / 48_000.0, 0.0, nominal);
        assert!((r / nominal - 1.0).abs() < 1e-9);
        // 上限の外
        assert!((follower_ratio(speed, 1e9 / RATE as f64, 0.5, 1.0) - tuning::MAX_RATIO_REL).abs() < 1e-12);
        assert!((follower_ratio(speed, 1e9 / RATE as f64, -0.5, 1.0) - 1.0 / tuning::MAX_RATIO_REL).abs() < 1e-12);
        assert_eq!(follower_ratio(0.0, 0.0, 0.0, 1.25), 1.25);
    }

    /// 開始時刻の門：今から 80ms 先の時刻で、その時刻以降の最初のフレームから鳴らす。
    /// 曲の位置は「時計 − 遅らせ量」。止めている時計なら位置は止めた所
    #[test]
    fn gate_opens_80ms_ahead_at_clock_minus_delay() {
        let mut t = StreamTiming::new(RATE);
        feed(&mut t, 1_000_000_000, 0.0, 100, |_| 0);
        let now = 1_500_000_000u64;
        let mut clock = Clock::new(RATE, 0.0);
        clock.start_at(now, 96_000.0);
        let g = plan_gate(&t, &clock, now, 0.0).unwrap();
        let lead = ms_to_ns(tuning::GATE_LEAD_MS) as f64;
        assert!(g.t_start_ns >= now as f64 + lead - 1.0);
        assert!(g.t_start_ns < now as f64 + lead + 1e9 / RATE as f64 + 1.0);
        // そのフレームは、開いた時刻から数えて 0.58 秒ぶん
        assert_eq!(g.k_start, ((0.5 + 0.08) * RATE as f64).ceil() as u64);
        let expect = 96_000.0 + (g.t_start_ns - now as f64) * Clock::nominal_speed(RATE);
        assert!((g.song_start - expect).abs() < 1e-6);
        // 遅らせ量 250ms
        let g2 = plan_gate(&t, &clock, now, delay_frames(250, RATE)).unwrap();
        assert!((g2.song_start - (expect - 12_000.0)).abs() < 1e-6);
        // 止めている時計
        let stopped = Clock::new(RATE, 1234.0);
        assert_eq!(plan_gate(&t, &stopped, now, 0.0).unwrap().song_start, 1234.0);
        // 記録が無ければ開けない
        assert!(plan_gate(&StreamTiming::new(RATE), &clock, now, 0.0).is_none());
    }

    /// 基準の選び方：揺れがいちばん小さいもの。差が 0.2ms 未満なら枠の番号が小さいほう
    #[test]
    fn reference_is_least_jitter_with_slot_tie_break() {
        assert_eq!(pick_reference(&[]), None);
        assert_eq!(pick_reference(&[(2, 0.03)]), Some(2));
        assert_eq!(pick_reference(&[(0, 0.50), (1, 0.03), (2, 0.27)]), Some(1));
        // 0.15ms の差は同じとみなす → 番号の小さい 0
        assert_eq!(pick_reference(&[(0, 0.18), (1, 0.03)]), Some(0));
        assert_eq!(pick_reference(&[(2, 0.10), (1, 0.35)]), Some(2));
        assert_eq!(pick_reference(&[(2, f64::NAN), (1, f64::NAN)]), Some(1));
    }

    /// つなぎ直しの間隔：0.5・1・2 秒、そのあと 5 秒おき。一覧に無い間は試さない
    #[test]
    fn retry_intervals() {
        assert_eq!(retry_delay_ms(0, true), Some(500));
        assert_eq!(retry_delay_ms(1, true), Some(1000));
        assert_eq!(retry_delay_ms(2, true), Some(2000));
        assert_eq!(retry_delay_ms(3, true), Some(5000));
        assert_eq!(retry_delay_ms(40, true), Some(5000));
        assert_eq!(retry_delay_ms(0, false), None);
        assert_eq!(retry_delay_ms(5, false), None);
    }

    /// 曲の終わり：読み切って、時計が「長さ＋遅らせ量の最大」に届いたとき
    #[test]
    fn track_end_waits_for_the_largest_delay() {
        assert!(!track_ended(false, 48_000, 1e9, 0.0));
        assert!(!track_ended(true, 48_000, 47_999.0, 0.0));
        assert!(track_ended(true, 48_000, 48_000.0, 0.0));
        let d = delay_frames(500, RATE);
        assert!(!track_ended(true, 48_000, 48_000.0, d));
        assert!(track_ended(true, 48_000, 72_000.0, d));
        // 遅らせ量は 1000ms で切る
        assert_eq!(delay_frames(5000, RATE), 48_000.0);
    }

    #[test]
    fn qpc_to_ns_rounds_to_100ns() {
        assert_eq!(qpc_to_ns(10_000_000, 10_000_000), 1_000_000_000);
        assert_eq!(qpc_to_ns(3, 10_000_000), 300);
        assert_eq!(qpc_to_ns(0, 10), 0);
        assert!(qpc_now_ns() > 0);
    }
}
