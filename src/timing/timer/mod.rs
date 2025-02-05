use crate::{
    comparison::personal_best, platform::prelude::*, util::PopulateString, AtomicDateTime, Run,
    Segment, Time, TimeSpan, TimeStamp, TimerPhase, TimerPhase::*, TimingMethod,
};
use core::{mem, ops::Deref};

#[cfg(test)]
mod tests;

pub type OnTimerChangeFunc = fn(&TimerState);
#[derive(Clone)]
pub struct OnTimerChange(OnTimerChangeFunc);

impl std::fmt::Debug for OnTimerChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("OnTimerChange")
            //.field("function", &std::any::type_name_of_val(&self.0))
            .field("function", &"user-defined-function")
            .finish()
    }
}

/// A `Timer` provides all the capabilities necessary for doing speedrun attempts.
///
/// # Examples
///
/// ```
/// use livesplit_core::{Run, Segment, Timer, TimerPhase};
///
/// // Create a run object that we can use with at least one segment.
/// let mut run = Run::new();
/// run.set_game_name("Super Mario Odyssey");
/// run.set_category_name("Any%");
/// run.push_segment(Segment::new("Cap Kingdom"));
///
/// // Create the timer from the run.
/// let mut timer = Timer::new(run).expect("Run with at least one segment provided");
///
/// // Start a new attempt.
/// timer.start();
/// assert_eq!(timer.current_phase(), TimerPhase::Running);
///
/// // Create a split.
/// timer.split();
///
/// // The run should be finished now.
/// assert_eq!(timer.current_phase(), TimerPhase::Ended);
///
/// // Reset the attempt and confirm that we want to store the attempt.
/// timer.reset(true);
///
/// // The attempt is now over.
/// assert_eq!(timer.current_phase(), TimerPhase::NotRunning);
/// ```
#[derive(Debug, Clone)]
pub struct Timer {
    run: Run,
    phase: TimerPhase,
    current_split_index: Option<usize>,
    current_timing_method: TimingMethod,
    current_comparison: String,
    attempt_started: Option<AtomicDateTime>,
    attempt_ended: Option<AtomicDateTime>,
    start_time: TimeStamp,
    start_time_with_offset: TimeStamp,
    // This gets adjusted after resuming
    adjusted_start_time: TimeStamp,
    time_paused_at: TimeSpan,
    is_game_time_paused: bool,
    game_time_pause_time: Option<TimeSpan>,
    loading_times: Option<TimeSpan>,
    start_time_utc: AtomicDateTime,
    start_time_with_offset_utc: AtomicDateTime,
    adjusted_start_time_utc: AtomicDateTime,
    use_utc: bool,
    on_timer_change: OnTimerChange,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum Action {
    #[default]
    None,
    Start,
    Split,
    Skip,
    Undo,
    Reset,
    Pause,
    Resume,
}
///
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerState {
    ///
    splits: Vec<Time64>,
    ///
    pub phase: String,
    ///
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_split_index: Option<usize>,
    ///
    pub current_timing_method: TimingMethod,
    ///
    pub current_comparison: String,
    ///
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt_started: Option<ADT>,
    ///
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt_ended: Option<ADT>,
    ///
    pub time_paused_at: f64,
    ///
    pub is_game_time_paused: bool,
    ///
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_time_pause_time: Option<f64>,
    ///
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loading_times: Option<f64>,
    //path: String,
    /// UTC TimeStamps
    start_time_utc: ADT,
    ///
    start_time_with_offset_utc: ADT,
    // This gets adjusted after resuming
    ///
    adjusted_start_time_utc: ADT,
    ///
    #[serde(default)]
    pub split_name: String,
    ///
    #[serde(default)]
    pub action: Action,
}

impl From<TimeSpan> for f64 {
    fn from(ts: TimeSpan) -> Self {
        ts.to_duration().as_seconds_f64()
    }
}
impl From<f64> for TimeSpan {
    fn from(v: f64) -> Self {
        TimeSpan::from_seconds(v)
    }
}
fn ts_to_f64(ts: TimeSpan) -> f64 {
    ts.to_duration().as_seconds_f64()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ADT {
    time: String,
    synced: bool,
}
use time::format_description::well_known::iso8601::Iso8601;

impl From<AtomicDateTime> for ADT {
    fn from(adt: AtomicDateTime) -> Self {
        Self {
            time: adt.time.format(&Iso8601::DEFAULT).unwrap(),
            synced: adt.synced(),
        }
    }
}

impl From<&ADT> for AtomicDateTime {
    fn from(adt: &ADT) -> Self {
        let dt = crate::DateTime::parse(&adt.time, &Iso8601::DEFAULT).unwrap();
        Self::new(dt, adt.synced)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Time64 {
    #[serde(skip_serializing_if = "Option::is_none")]
    real_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_time: Option<f64>,
}
impl From<Time> for Time64 {
    fn from(item: Time) -> Self {
        Self {
            real_time: item.real_time.map(ts_to_f64),
            game_time: item.game_time.map(ts_to_f64),
        }
    }
}
impl From<&Time64> for Time {
    fn from(item: &Time64) -> Self {
        Time::new()
            .with_real_time(item.real_time.map(|x| x.into()))
            .with_game_time(item.game_time.map(|x| x.into()))
    }
}
impl From<&str> for TimerPhase {
    fn from(val: &str) -> Self {
        if val == "NotRunning" {
            TimerPhase::NotRunning
        } else if val == "Running" {
            TimerPhase::Running
        } else if val == "Ended" {
            TimerPhase::Ended
        } else if val == "Paused" {
            TimerPhase::Paused
        } else {
            panic!("Unknown TimerPhase value: {}", val);
        }
    }
}

impl From<&Timer> for TimerState {
    fn from(timer: &Timer) -> Self {
        let splits = timer
            .run
            .segments()
            .iter()
            .map(|seg| seg.split_time().into())
            .collect();
        let split_name = match timer.current_split() {
            Some(seg) => seg.name().to_string(),
            None => "empty".to_string(),
        };
        TimerState {
            splits: splits,
            phase: format!("{:?}", timer.phase),
            current_split_index: timer.current_split_index,
            current_timing_method: timer.current_timing_method,
            current_comparison: timer.current_comparison.clone(),
            attempt_started: timer.attempt_started.map(|x| x.into()),
            attempt_ended: timer.attempt_ended.map(|x| x.into()),
            time_paused_at: ts_to_f64(timer.time_paused_at),
            is_game_time_paused: timer.is_game_time_paused,
            game_time_pause_time: timer.game_time_pause_time.map(ts_to_f64),
            loading_times: timer.loading_times.map(ts_to_f64),
            start_time_utc: timer.start_time_utc.into(),
            start_time_with_offset_utc: timer.start_time_with_offset_utc.into(),
            adjusted_start_time_utc: timer.adjusted_start_time_utc.into(),
            split_name,
            action: Action::None,
        }
    }
}

impl TimerState {
    /// from file
    pub fn from_file(path: &str) -> Option<Self> {
        if let Ok(data) = std::fs::read(path) {
            Some(serde_json::from_slice(&data).unwrap())
        } else {
            None
        }
    }
    /// to_json
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).unwrap()
    }
}

/// A snapshot represents a specific point in time that the timer was observed
/// at. The snapshot dereferences to the timer. Everything you perceive through
/// the snapshot is entirely frozen in time.
pub struct Snapshot<'timer> {
    timer: &'timer Timer,
    time: Time,
}

impl Snapshot<'_> {
    /// Returns the time the timer was at when the snapshot was taken. The Game
    /// Time is None if the Game Time has not been initialized.
    pub const fn current_time(&self) -> Time {
        self.time
    }
}

impl Deref for Snapshot<'_> {
    type Target = Timer;
    fn deref(&self) -> &Self::Target {
        self.timer
    }
}

/// A `SharedTimer` is a wrapper around the [`Timer`](crate::timing::Timer) that can be shared across multiple threads with multiple owners.
#[cfg(feature = "std")]
pub type SharedTimer = alloc::sync::Arc<std::sync::RwLock<Timer>>;

/// The Error type for creating a new Timer from a Run.
#[derive(Debug, snafu::Snafu)]
pub enum CreationError {
    /// The Timer couldn't be created, because the Run has no segments.
    EmptyRun,
}

impl Timer {
    /// Creates a new Timer based on a Run object storing all the information
    /// about the splits. The Run object needs to have at least one segment, so
    /// that the Timer can store the final time. If a Run object with no
    /// segments is provided, the Timer creation fails.
    #[inline]
    pub fn new(mut run: Run) -> Result<Self, CreationError> {
        if run.is_empty() {
            return Err(CreationError::EmptyRun);
        }

        run.fix_splits();
        run.regenerate_comparisons();
        let now = TimeStamp::now();
        let now_utc = AtomicDateTime::now();

        Ok(Timer {
            run,
            phase: NotRunning,
            current_split_index: None,
            current_timing_method: TimingMethod::RealTime,
            current_comparison: personal_best::NAME.into(),
            attempt_started: None,
            attempt_ended: None,
            start_time: now,
            start_time_with_offset: now,
            adjusted_start_time: now,
            time_paused_at: TimeSpan::zero(),
            is_game_time_paused: false,
            game_time_pause_time: None,
            loading_times: None,
            start_time_utc: now_utc,
            start_time_with_offset_utc: now_utc,
            adjusted_start_time_utc: now_utc,
            use_utc: true,
            on_timer_change: OnTimerChange(Timer::on_timer_change_noop),
        })
    }

    /// Consumes the Timer and creates a Shared Timer that can be shared across
    /// multiple threads with multiple owners.
    #[cfg(feature = "std")]
    pub fn into_shared(self) -> SharedTimer {
        alloc::sync::Arc::new(std::sync::RwLock::new(self))
    }

    ///
    pub fn use_utc(&mut self, use_utc: bool) {
        self.use_utc = use_utc;
    }

    /// Takes out the Run from the Timer and resets the current attempt if there
    /// is one in progress. If the splits are to be updated, all the information
    /// of the current attempt is stored in the Run's history. Otherwise the
    /// current attempt's information is discarded.
    pub fn into_run(mut self, update_splits: bool) -> Run {
        self.reset(update_splits);
        self.run
    }

    /// Replaces the Run object used by the Timer with the Run object provided.
    /// If the Run provided contains no segments, it can't be used for timing
    /// and is returned as the `Err` case of the `Result`. Otherwise the Run
    /// that was in use by the Timer is being returned. Before the Run is
    /// returned, the current attempt is reset and the splits are being updated
    /// depending on the `update_splits` parameter.
    pub fn replace_run(&mut self, mut run: Run, update_splits: bool) -> Result<Run, Run> {
        if run.is_empty() {
            return Err(run);
        }

        self.reset(update_splits);
        if !run.comparisons().any(|c| c == self.current_comparison) {
            self.current_comparison = personal_best::NAME.to_string();
        }

        run.fix_splits();
        run.regenerate_comparisons();

        Ok(mem::replace(&mut self.run, run))
    }

    /// Sets the Run object used by the Timer with the Run object provided. If
    /// the Run provided contains no segments, it can't be used for timing and
    /// is returned as the Err case of the Result. The Run object in use by the
    /// Timer is dropped by this method.
    pub fn set_run(&mut self, run: Run) -> Result<(), Run> {
        self.replace_run(run, false).map(drop)
    }

    /// Accesses the Run in use by the Timer.
    #[inline]
    pub const fn run(&self) -> &Run {
        &self.run
    }

    /// Marks the Run as unmodified, so that it is known that all the changes
    /// have been saved.
    #[inline]
    pub fn mark_as_unmodified(&mut self) {
        self.run.mark_as_unmodified();
    }

    /// Returns the current Timer Phase.
    #[inline]
    pub const fn current_phase(&self) -> TimerPhase {
        self.phase
    }

    fn current_time(&self) -> Time {
        let t0 = TimeStamp::now();
        let t0_utc = AtomicDateTime::now();
        let real_time = match self.phase {
            NotRunning => Some(self.run.offset()),
            Running => Some(t0 - self.adjusted_start_time),
            Paused => Some(self.time_paused_at),
            Ended => self.run.segments().last().unwrap().split_time().real_time,
        };
        let real_time_utc = match self.phase {
            NotRunning => Some(self.run.offset()),
            Running => Some(t0_utc - self.adjusted_start_time_utc),
            Paused => Some(self.time_paused_at),
            Ended => self.run.segments().last().unwrap().split_time().real_time,
        };

        let game_time = match self.phase {
            NotRunning => Some(self.run.offset()),
            Ended => self.run.segments().last().unwrap().split_time().game_time,
            _ => {
                if self.is_game_time_paused() {
                    self.game_time_pause_time
                } else if self.is_game_time_initialized() {
                    catch! { real_time? - self.loading_times() }
                } else {
                    None
                }
            }
        };

        Time::new()
            .with_real_time(if self.use_utc {
                real_time_utc
            } else {
                real_time
            })
            .with_game_time(game_time)
    }

    /// Creates a new snapshot of the timer at the point in time of this call.
    /// It represents a frozen state of the timer such that calculations can
    /// work with an entirely consistent view of the timer without the current
    /// time changing underneath.
    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot {
            timer: self,
            time: self.current_time(),
        }
    }

    /// Returns the currently selected Timing Method.
    #[inline]
    pub const fn current_timing_method(&self) -> TimingMethod {
        self.current_timing_method
    }

    /// Sets the current Timing Method to the Timing Method provided.
    #[inline]
    pub fn set_current_timing_method(&mut self, method: TimingMethod) {
        self.current_timing_method = method;
    }

    /// Toggles between the `Real Time` and `Game Time` timing methods.
    #[inline]
    pub fn toggle_timing_method(&mut self) {
        self.current_timing_method = match self.current_timing_method {
            TimingMethod::RealTime => TimingMethod::GameTime,
            TimingMethod::GameTime => TimingMethod::RealTime,
        };
    }

    /// Returns the current comparison that is being compared against. This may
    /// be a custom comparison or one of the Comparison Generators.
    #[inline]
    pub fn current_comparison(&self) -> &str {
        &self.current_comparison
    }

    /// Tries to set the current comparison to the comparison specified. If the
    /// comparison doesn't exist `Err` is returned.
    #[inline]
    pub fn set_current_comparison<S: PopulateString>(&mut self, comparison: S) -> Result<(), ()> {
        let as_str = comparison.as_str();
        if self.run.comparisons().any(|c| c == as_str) {
            comparison.populate(&mut self.current_comparison);
            Ok(())
        } else {
            Err(())
        }
    }

    /// Accesses the split the attempt is currently on. If there's no attempt in
    /// progress or the run finished, `None` is returned instead.
    pub fn current_split(&self) -> Option<&Segment> {
        self.current_split_index
            .and_then(|i| self.run.segments().get(i))
    }

    fn current_split_mut(&mut self) -> Option<&mut Segment> {
        self.current_split_index
            .and_then(move |i| self.run.segments_mut().get_mut(i))
    }

    /// Accesses the index of the split the attempt is currently on. If there's
    /// no attempt in progress, `None` is returned instead. This returns an
    /// index that is equal to the amount of segments when the attempt is
    /// finished, but has not been reset. So you need to be careful when using
    /// this value for indexing.
    #[inline]
    pub const fn current_split_index(&self) -> Option<usize> {
        self.current_split_index
    }

    /// Starts the Timer if there is no attempt in progress. If that's not the
    /// case, nothing happens.
    pub fn start(&mut self) {
        let t0 = TimeStamp::now();
        let t0_utc = AtomicDateTime::now();
        if self.phase == NotRunning {
            self.phase = Running;
            self.current_split_index = Some(0);
            self.attempt_started = Some(AtomicDateTime::now());
            self.start_time = t0;
            self.start_time_utc = t0_utc;
            self.start_time_with_offset = self.start_time - self.run.offset();
            self.adjusted_start_time = self.start_time_with_offset;
            self.time_paused_at = self.run.offset();
            self.deinitialize_game_time();
            self.run.start_next_run();

            self.start_time_with_offset_utc = self.start_time_utc - self.run.offset();
            self.adjusted_start_time_utc = self.start_time_with_offset_utc;
            // FIXME: OnStart
            self.save_state(Action::Start);
        }
    }

    /// If an attempt is in progress, stores the current time as the time of the
    /// current split. The attempt ends if the last split time is stored.
    pub fn split(&mut self) {
        let current_time = self.current_time();
        if self.phase == Running
            && current_time
                .real_time
                .map_or(false, |t| t >= TimeSpan::zero())
        {
            // FIXME: We shouldn't need to collect here.
            let variables = self
                .run
                .metadata()
                .custom_variables()
                .map(|(k, v)| (k.to_owned(), v.value.clone()))
                .collect();
            let segment = self.current_split_mut().unwrap();

            segment.set_split_time(current_time);
            *segment.variables_mut() = variables;

            *self.current_split_index.as_mut().unwrap() += 1;
            if Some(self.run.len()) == self.current_split_index {
                self.phase = Ended;
                self.attempt_ended = Some(AtomicDateTime::now());
            }
            self.run.mark_as_modified();
            self.save_state(Action::Split);
            // FIXME: OnSplit
        }
    }

    ///
    fn on_timer_change_noop(_timer_state: &TimerState) {}

    ///
    pub fn set_on_timer_change(&mut self, func: OnTimerChangeFunc) {
        self.on_timer_change = OnTimerChange(func);
    }
    ///
    pub fn save_state(&self, action: Action) {
        let func = self.on_timer_change.0;
        func(&self.timer_state(action));
    }
    ///
    pub fn timer_state(&self, action: Action) -> TimerState {
        let mut state: TimerState = self.into();
        state.action = action;
        state
    }
    ///
    pub fn replace_state(&mut self, state: &TimerState) {
        if state.splits.len() != self.run.segments().len() {
            panic!(
                "inconsistent state, run has {} segments, state has {} segments",
                self.run.segments().len(),
                state.splits.len()
            );
        }
        for (i, split) in state.splits.iter().enumerate() {
            self.run.segment_mut(i).set_split_time(split.into());
        }
        self.phase = state.phase.as_str().into();
        self.current_split_index = state.current_split_index;
        self.current_timing_method = state.current_timing_method;
        self.current_comparison = state.current_comparison.clone();
        self.attempt_started = state.attempt_started.as_ref().map(|x| x.into());
        self.attempt_ended = state.attempt_started.as_ref().map(|x| x.into());
        self.start_time = TimeStamp::now();
        self.start_time_with_offset = self.start_time;
        self.adjusted_start_time = self.start_time;
        self.time_paused_at = state.time_paused_at.into();
        self.is_game_time_paused = state.is_game_time_paused;
        self.game_time_pause_time = state.game_time_pause_time.map(|x| x.into());
        self.loading_times = state.loading_times.map(|x| x.into());
        self.start_time_utc = (&state.start_time_utc).into();
        self.start_time_with_offset_utc = (&state.start_time_with_offset_utc).into();
        self.adjusted_start_time_utc = (&state.adjusted_start_time_utc).into();
    }

    /// Starts a new attempt or stores the current time as the time of the
    /// current split. The attempt ends if the last split time is stored.
    pub fn split_or_start(&mut self) {
        if self.phase == NotRunning {
            self.start();
        } else {
            self.split();
        }
    }

    /// Skips the current split if an attempt is in progress and the
    /// current split is not the last split.
    pub fn skip_split(&mut self) {
        if (self.phase == Running || self.phase == Paused)
            && self.current_split_index < self.run.len().checked_sub(1)
        {
            self.current_split_mut().unwrap().clear_split_info();

            self.current_split_index = self.current_split_index.map(|i| i + 1);
            self.run.mark_as_modified();
            self.save_state(Action::Skip);
            // FIXME: OnSkipSplit
        }
    }

    /// Removes the split time from the last split if an attempt is in progress
    /// and there is a previous split. The Timer Phase also switches to
    /// `Running` if it previously was `Ended`.
    pub fn undo_split(&mut self) {
        if self.phase != NotRunning && self.current_split_index > Some(0) {
            if self.phase == Ended {
                self.phase = Running;
            }
            self.current_split_index = self.current_split_index.map(|i| i - 1);

            self.current_split_mut().unwrap().clear_split_info();

            self.run.mark_as_modified();
            self.save_state(Action::Undo);
            // FIXME: OnUndoSplit
        }
    }

    /// Resets the current attempt if there is one in progress. If the splits
    /// are to be updated, all the information of the current attempt is stored
    /// in the Run's history. Otherwise the current attempt's information is
    /// discarded.
    pub fn reset(&mut self, update_splits: bool) {
        if self.phase != NotRunning {
            self.reset_state(update_splits);
            self.reset_splits();
            self.save_state(Action::Reset);
        }
    }

    /// Resets the current attempt if there is one in progress. The splits are
    /// updated such that the current attempt's split times are being stored as
    /// the new Personal Best.
    pub fn reset_and_set_attempt_as_pb(&mut self) {
        if self.phase != NotRunning {
            self.reset_state(true);
            self.set_run_as_pb();
            self.reset_splits();
        }
    }

    fn reset_state(&mut self, update_times: bool) {
        if self.phase != Ended {
            self.attempt_ended = Some(AtomicDateTime::now());
        }
        self.resume_game_time();
        self.set_loading_times(TimeSpan::zero());

        if update_times {
            self.update_attempt_history();
            self.update_best_segments();
            self.update_pb_splits();
            self.update_segment_history();
        }
    }

    fn reset_splits(&mut self) {
        self.phase = NotRunning;
        self.current_split_index = None;

        // Reset Splits
        for segment in self.run.segments_mut() {
            segment.clear_split_info();
        }

        // FIXME: OnReset

        self.run.fix_splits();
        self.run.regenerate_comparisons();
    }

    /// Pauses an active attempt that is not paused.
    pub fn pause(&mut self) {
        if self.phase == Running {
            self.time_paused_at = self.current_time().real_time.unwrap();
            self.phase = Paused;
            self.save_state(Action::Pause);
            // FIXME: OnPause
        }
    }

    /// Resumes an attempt that is paused.
    pub fn resume(&mut self) {
        if self.phase == Paused {
            self.adjusted_start_time = TimeStamp::now() - self.time_paused_at;
            self.adjusted_start_time_utc = AtomicDateTime::now() - self.time_paused_at;
            self.phase = Running;
            self.save_state(Action::Resume);
            // FIXME: OnResume
        }
    }

    /// Toggles an active attempt between `Paused` and `Running`.
    pub fn toggle_pause(&mut self) {
        match self.phase {
            Running => self.pause(),
            Paused => self.resume(),
            _ => {}
        }
    }

    /// Toggles an active attempt between `Paused` and `Running` or starts an
    /// attempt if there's none in progress.
    pub fn toggle_pause_or_start(&mut self) {
        match self.phase {
            Running => self.pause(),
            Paused => self.resume(),
            NotRunning => self.start(),
            _ => {}
        }
    }

    /// Removes all the pause times from the current time. If the current
    /// attempt is paused, it also resumes that attempt. Additionally, if the
    /// attempt is finished, the final split time is adjusted to not include the
    /// pause times as well.
    ///
    /// # Warning
    ///
    /// This behavior is not entirely optimal, as generally only the final split
    /// time is modified, while all other split times are left unmodified, which
    /// may not be what actually happened during the run.
    pub fn undo_all_pauses(&mut self) {
        match self.current_phase() {
            Paused => self.resume(),
            Ended => {
                let pause_time = Some(self.get_pause_time().unwrap_or_default());

                let split_time = self
                    .run
                    .segments_mut()
                    .iter_mut()
                    .last()
                    .unwrap()
                    .split_time_mut();

                *split_time += Time::new()
                    .with_real_time(pause_time)
                    .with_game_time(pause_time);
            }
            _ => {}
        }

        self.adjusted_start_time = self.start_time_with_offset;
        self.adjusted_start_time_utc = self.start_time_with_offset_utc;

        // FIXME: OnUndoAllPauses
    }

    /// Switches the current comparison to the next comparison in the list.
    pub fn switch_to_next_comparison(&mut self) {
        let mut comparisons = self.run.comparisons();
        let len = comparisons.len();
        let index = comparisons
            .position(|c| c == self.current_comparison)
            .unwrap();
        let index = (index + 1) % len;
        self.current_comparison = self.run.comparisons().nth(index).unwrap().to_owned();

        // FIXME: OnNextComparison
    }

    /// Switches the current comparison to the previous comparison in the list.
    pub fn switch_to_previous_comparison(&mut self) {
        let mut comparisons = self.run.comparisons();
        let len = comparisons.len();
        let index = comparisons
            .position(|c| c == self.current_comparison)
            .unwrap();
        let index = (index + len - 1) % len;
        self.current_comparison = self.run.comparisons().nth(index).unwrap().to_owned();

        // FIXME: OnPreviousComparison
    }

    /// Returns the total duration of the current attempt. This is not affected
    /// by the start offset of the run. So if the start offset is -10s and the
    /// `start()` method was called 2s ago, the current time is -8s but the
    /// current attempt duration is 2s. If the timer is then however paused for
    /// 5s, the current attempt duration is still 2s. So the current attempt
    /// duration only counts the time the Timer Phase has actually been
    /// `Running`.
    pub fn current_attempt_duration(&self) -> TimeSpan {
        let t0 = TimeStamp::now();
        let t0_utc = AtomicDateTime::now();
        let ts = match self.current_phase() {
            NotRunning => TimeSpan::zero(),
            Paused | Running => t0 - self.start_time,
            Ended => self.attempt_ended.unwrap() - self.attempt_started.unwrap(),
        };
        let ts_utc = match self.current_phase() {
            NotRunning => TimeSpan::zero(),
            Paused | Running => t0_utc - self.start_time_utc,
            Ended => self.attempt_ended.unwrap() - self.attempt_started.unwrap(),
        };
        dbg!(self.current_phase(), self.start_time_utc);
        dbg!(ts, ts_utc, self.use_utc);
        if self.use_utc {
            ts_utc
        } else {
            ts
        }
    }

    /// Returns the total amount of time the current attempt has been paused
    /// for. None is returned if there have not been any pauses.
    pub fn get_pause_time(&self) -> Option<TimeSpan> {
        let t0 = TimeStamp::now();
        let t0_utc = AtomicDateTime::now();
        let pt = match self.current_phase() {
            Paused => Some(t0 - self.start_time_with_offset - self.time_paused_at),
            Running | Ended if self.start_time_with_offset != self.adjusted_start_time => {
                Some(self.adjusted_start_time - self.start_time_with_offset)
            }
            _ => None,
        };
        let pt_utc = match self.current_phase() {
            Paused => Some(t0_utc - self.start_time_with_offset_utc - self.time_paused_at),
            Running | Ended if self.start_time_with_offset_utc != self.adjusted_start_time_utc => {
                Some(self.adjusted_start_time_utc - self.start_time_with_offset_utc)
            }
            _ => None,
        };
        if self.use_utc {
            pt_utc
        } else {
            pt
        }
    }

    /// Returns whether Game Time is currently initialized. Game Time
    /// automatically gets uninitialized for each new attempt.
    #[inline]
    pub const fn is_game_time_initialized(&self) -> bool {
        self.loading_times.is_some()
    }

    /// Initializes Game Time for the current attempt. Game Time automatically
    /// gets uninitialized for each new attempt.
    #[inline]
    pub fn initialize_game_time(&mut self) {
        self.loading_times = Some(self.loading_times());
    }

    /// Deinitializes Game Time for the current attempt.
    #[inline]
    pub fn deinitialize_game_time(&mut self) {
        self.loading_times = None;
    }

    /// Returns whether the Game Timer is currently paused. If the Game Timer is
    /// not paused, it automatically increments similar to Real Time.
    #[inline]
    pub const fn is_game_time_paused(&self) -> bool {
        self.is_game_time_paused
    }

    /// Pauses the Game Timer such that it doesn't automatically increment
    /// similar to Real Time.
    pub fn pause_game_time(&mut self) {
        if !self.is_game_time_paused() {
            let current_time = self.current_time();
            self.game_time_pause_time = current_time.game_time.or(current_time.real_time);
            self.is_game_time_paused = true;
        }
    }

    /// Resumes the Game Timer such that it automatically increments similar to
    /// Real Time, starting from the Game Time it was paused at.
    pub fn resume_game_time(&mut self) {
        if self.is_game_time_paused() {
            let current_time = self.current_time();
            let diff = catch! { current_time.real_time? - current_time.game_time? };
            self.set_loading_times(diff.unwrap_or_default());
            self.is_game_time_paused = false;
        }
    }

    /// Sets the Game Time to the time specified. This also works if the Game
    /// Time is paused, which can be used as a way of updating the Game Timer
    /// periodically without it automatically moving forward. This ensures that
    /// the Game Timer never shows any time that is not coming from the game.
    #[inline]
    pub fn set_game_time(&mut self, game_time: TimeSpan) {
        if self.is_game_time_paused() {
            self.game_time_pause_time = Some(game_time);
        }
        self.loading_times = Some(self.current_time().real_time.unwrap() - game_time);
    }

    /// Accesses the loading times. Loading times are defined as Game Time - Real Time.
    #[inline]
    pub fn loading_times(&self) -> TimeSpan {
        self.loading_times.unwrap_or_default()
    }

    /// Instead of setting the Game Time directly, this method can be used to
    /// just specify the amount of time the game has been loading. The Game Time
    /// is then automatically determined by Real Time - Loading Times.
    #[inline]
    pub fn set_loading_times(&mut self, time: TimeSpan) {
        self.loading_times = Some(time);
        if self.is_game_time_paused() {
            self.game_time_pause_time = Some(self.current_time().real_time.unwrap() - time);
        }
    }

    /// Sets the value of a custom variable with the name specified. If the
    /// variable does not exist, a temporary variable gets created that will not
    /// be stored in the splits file.
    pub fn set_custom_variable<N, V>(&mut self, name: N, value: V)
    where
        N: PopulateString,
        V: PopulateString,
    {
        let var = self.run.metadata_mut().custom_variable_mut(name);
        var.set_value(value);
        if var.is_permanent {
            self.run.mark_as_modified();
        }
    }

    fn update_attempt_history(&mut self) {
        let time = if self.phase == Ended {
            self.current_time()
        } else {
            Default::default()
        };

        let pause_time = self.get_pause_time();

        self.run
            .add_attempt(time, self.attempt_started, self.attempt_ended, pause_time);
    }

    fn update_best_segments(&mut self) {
        let mut previous_split_time_rta = Some(TimeSpan::zero());
        let mut previous_split_time_game_time = Some(TimeSpan::zero());

        for split in self.run.segments_mut() {
            let mut new_best_segment = split.best_segment_time();
            if let Some(split_time) = split.split_time().real_time {
                let current_segment = previous_split_time_rta.map(|previous| split_time - previous);
                previous_split_time_rta = Some(split_time);
                if split
                    .best_segment_time()
                    .real_time
                    .map_or(true, |b| current_segment.map_or(false, |c| c < b))
                {
                    new_best_segment.real_time = current_segment;
                }
            }
            if let Some(split_time) = split.split_time().game_time {
                let current_segment =
                    previous_split_time_game_time.map(|previous| split_time - previous);
                previous_split_time_game_time = Some(split_time);
                if split
                    .best_segment_time()
                    .game_time
                    .map_or(true, |b| current_segment.map_or(false, |c| c < b))
                {
                    new_best_segment.game_time = current_segment;
                }
            }
            split.set_best_segment_time(new_best_segment);
        }
    }

    fn update_pb_splits(&mut self) {
        let method = self.current_timing_method;
        let (split_time, pb_split_time) = {
            let last_segment = self.run.segments().last().unwrap();
            (
                last_segment.split_time()[method],
                last_segment.personal_best_split_time()[method],
            )
        };
        if split_time.map_or(false, |s| pb_split_time.map_or(true, |pb| s < pb)) {
            self.set_run_as_pb();
        }
    }

    fn update_segment_history(&mut self) {
        if let Some(index) = self.current_split_index {
            self.run.update_segment_history(index);
        }
    }

    fn set_run_as_pb(&mut self) {
        self.run.import_pb_into_segment_history();
        self.run.fix_splits();
        for segment in self.run.segments_mut() {
            let split_time = segment.split_time();
            segment.set_personal_best_split_time(split_time);
        }
        self.run.clear_run_id();
    }
}
