use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub last_cleanup: Option<DateTime<Utc>>,
    pub confirm_before_clean: bool,
    pub show_cleanup_summary: bool,
    #[serde(default)]
    pub scheduled_cleanup_enabled: bool,
    #[serde(default)]
    pub scheduled_cleanup_day: ScheduleDay,
    /// Multi-day schedule. `None` means this settings file predates the
    /// multi-select UI and should fall back to `scheduled_cleanup_day`.
    #[serde(default)]
    pub scheduled_cleanup_days: Option<Vec<ScheduleDay>>,
    #[serde(default = "default_scheduled_cleanup_hour")]
    pub scheduled_cleanup_hour: u8,
    #[serde(default)]
    pub scheduled_cleanup_minute: u8,
    /// When the last scheduled cleanup started, or when the schedule was
    /// last changed. A slot that passed after this instant is still due, so
    /// cleanups missed while the computer was off or asleep run later.
    #[serde(default)]
    pub last_scheduled_cleanup: Option<DateTime<Utc>>,
    #[serde(default)]
    pub application_log_cleanup_enabled: bool,
    #[serde(default)]
    pub system_journal_cleanup_enabled: bool,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
    pub max_files_per_operation: usize,
    /// Maximum in bytes
    pub max_size_per_operation: u64,
    pub follow_symlinks: bool,
    pub verbose_logging: bool,
    pub color_scheme: ColorScheme,
    #[serde(default)]
    pub language: AppLanguage,
    #[serde(default = "default_check_for_updates")]
    pub check_for_updates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorScheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum AppLanguage {
    #[default]
    Automatic = 0,
    English = 1,
    Greek = 2,
    Italian = 3,
    Spanish = 4,
    German = 5,
    French = 6,
}

impl AppLanguage {
    pub const fn settings_index(self) -> u32 {
        self as u32
    }

    pub const fn from_settings_index(index: u32) -> Self {
        match index {
            1 => Self::English,
            2 => Self::Greek,
            3 => Self::Italian,
            4 => Self::Spanish,
            5 => Self::German,
            6 => Self::French,
            _ => Self::Automatic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScheduleDay {
    #[default]
    EveryDay,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl ScheduleDay {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::EveryDay => "Every day",
            Self::Monday => "Monday",
            Self::Tuesday => "Tuesday",
            Self::Wednesday => "Wednesday",
            Self::Thursday => "Thursday",
            Self::Friday => "Friday",
            Self::Saturday => "Saturday",
            Self::Sunday => "Sunday",
        }
    }

    pub fn weekdays() -> &'static [Self] {
        &[
            Self::Monday,
            Self::Tuesday,
            Self::Wednesday,
            Self::Thursday,
            Self::Friday,
            Self::Saturday,
            Self::Sunday,
        ]
    }

    pub fn matches(self, weekday: chrono::Weekday) -> bool {
        match self {
            Self::EveryDay => true,
            Self::Monday => weekday == chrono::Weekday::Mon,
            Self::Tuesday => weekday == chrono::Weekday::Tue,
            Self::Wednesday => weekday == chrono::Weekday::Wed,
            Self::Thursday => weekday == chrono::Weekday::Thu,
            Self::Friday => weekday == chrono::Weekday::Fri,
            Self::Saturday => weekday == chrono::Weekday::Sat,
            Self::Sunday => weekday == chrono::Weekday::Sun,
        }
    }
}

impl AppSettings {
    /// Upper bounds for the safety limits exposed in the Settings page.
    /// Large disks are common now, so the data limit goes to 32 TiB rather
    /// than the old 1 TiB ceiling that blocked cleaning full multi-TB disks.
    pub const MAX_FILES_PER_OPERATION_LIMIT: usize = 100_000_000;
    pub const MAX_SIZE_PER_OPERATION_LIMIT: u64 = 32 * 1024u64.pow(4);

    pub fn effective_scheduled_cleanup_days(&self) -> Vec<ScheduleDay> {
        if let Some(saved_days) = self.scheduled_cleanup_days.as_ref() {
            if saved_days.contains(&ScheduleDay::EveryDay) {
                return ScheduleDay::weekdays().to_vec();
            }

            let selected: Vec<_> = ScheduleDay::weekdays()
                .iter()
                .copied()
                .filter(|day| saved_days.contains(day))
                .collect();
            if !selected.is_empty() {
                return selected;
            }
        }

        if self.scheduled_cleanup_day == ScheduleDay::EveryDay {
            ScheduleDay::weekdays().to_vec()
        } else {
            vec![self.scheduled_cleanup_day]
        }
    }

    pub fn set_scheduled_cleanup_days(&mut self, days: Vec<ScheduleDay>) {
        let selected: Vec<_> = ScheduleDay::weekdays()
            .iter()
            .copied()
            .filter(|day| days.contains(day))
            .collect();
        if selected.is_empty() {
            return;
        }

        self.scheduled_cleanup_day = if selected.len() == ScheduleDay::weekdays().len() {
            ScheduleDay::EveryDay
        } else {
            selected[0]
        };
        self.scheduled_cleanup_days = Some(selected);
    }

    /// Restart the catch-up window after the schedule is enabled or edited,
    /// so a change never triggers a cleanup for a slot that already passed.
    pub fn mark_schedule_changed(&mut self) {
        self.last_scheduled_cleanup = Some(Utc::now());
    }

    /// The most recent scheduled slot at or before `now`, in `now`'s time
    /// zone. Days whose local time does not exist, such as a daylight saving
    /// gap, are skipped.
    pub fn latest_scheduled_slot<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let today = now.date_naive();
        (0..=7).find_map(|days_back| {
            let date = today - Duration::days(days_back);
            if !self.scheduled_cleanup_matches(date.weekday()) {
                return None;
            }
            let local = date.and_hms_opt(
                u32::from(self.scheduled_cleanup_hour),
                u32::from(self.scheduled_cleanup_minute),
                0,
            )?;
            now.timezone()
                .from_local_datetime(&local)
                .earliest()
                .filter(|slot| slot <= now)
        })
    }

    /// Whether a scheduled cleanup is due: the schedule is on and a slot has
    /// passed since the last run. Several missed slots still mean one run.
    pub fn scheduled_cleanup_due<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> bool {
        if !self.scheduled_cleanup_enabled {
            return false;
        }
        let (Some(last_run), Some(slot)) =
            (self.last_scheduled_cleanup, self.latest_scheduled_slot(now))
        else {
            return false;
        };
        slot.with_timezone(&Utc) > last_run
    }

    pub fn scheduled_cleanup_matches(&self, weekday: chrono::Weekday) -> bool {
        self.effective_scheduled_cleanup_days()
            .iter()
            .any(|day| day.matches(weekday))
    }

    /// Clamp safety limits to sensible bounds.
    pub fn clamp_values(&mut self) {
        self.max_files_per_operation = self
            .max_files_per_operation
            .clamp(100, Self::MAX_FILES_PER_OPERATION_LIMIT);
        // Directory symlink traversal was removed because it can escape the
        // scanned cleanup root through an intermediate path component.
        self.follow_symlinks = false;
        self.max_size_per_operation = self
            .max_size_per_operation
            .clamp(1024 * 1024, Self::MAX_SIZE_PER_OPERATION_LIMIT);
        self.scheduled_cleanup_hour = self.scheduled_cleanup_hour.min(23);
        self.scheduled_cleanup_minute = self.scheduled_cleanup_minute.min(59);
        self.log_retention_days = self.log_retention_days.clamp(1, 3650);
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            last_cleanup: None,
            confirm_before_clean: true,
            show_cleanup_summary: true,
            scheduled_cleanup_enabled: false,
            scheduled_cleanup_day: ScheduleDay::EveryDay,
            scheduled_cleanup_days: Some(ScheduleDay::weekdays().to_vec()),
            scheduled_cleanup_hour: default_scheduled_cleanup_hour(),
            scheduled_cleanup_minute: 0,
            last_scheduled_cleanup: None,
            application_log_cleanup_enabled: false,
            system_journal_cleanup_enabled: false,
            log_retention_days: default_log_retention_days(),
            max_files_per_operation: 10000,
            max_size_per_operation: 10 * 1024 * 1024 * 1024, // 10 GB
            follow_symlinks: false,
            verbose_logging: false,
            color_scheme: ColorScheme::System,
            language: AppLanguage::Automatic,
            check_for_updates: default_check_for_updates(),
        }
    }
}

const fn default_scheduled_cleanup_hour() -> u8 {
    3
}

const fn default_log_retention_days() -> u32 {
    30
}

const fn default_check_for_updates() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_values() {
        let mut settings = AppSettings {
            max_files_per_operation: 5000,
            max_size_per_operation: 5 * 1024 * 1024 * 1024,
            ..Default::default()
        };

        // Values within bounds should not change
        settings.clamp_values();
        assert_eq!(settings.max_files_per_operation, 5000);
        assert_eq!(settings.max_size_per_operation, 5 * 1024 * 1024 * 1024);

        // Values below minimum should be clamped up
        settings.max_files_per_operation = 50;
        settings.max_size_per_operation = 1024;
        settings.clamp_values();
        assert_eq!(settings.max_files_per_operation, 100);
        assert_eq!(settings.max_size_per_operation, 1024 * 1024);

        // Values above maximum should be clamped down
        settings.max_files_per_operation = AppSettings::MAX_FILES_PER_OPERATION_LIMIT + 1;
        settings.max_size_per_operation = AppSettings::MAX_SIZE_PER_OPERATION_LIMIT + 1;
        settings.clamp_values();
        assert_eq!(
            settings.max_files_per_operation,
            AppSettings::MAX_FILES_PER_OPERATION_LIMIT
        );
        assert_eq!(
            settings.max_size_per_operation,
            AppSettings::MAX_SIZE_PER_OPERATION_LIMIT
        );

        settings.scheduled_cleanup_hour = 99;
        settings.scheduled_cleanup_minute = 99;
        settings.log_retention_days = 0;
        settings.clamp_values();
        assert_eq!(settings.scheduled_cleanup_hour, 23);
        assert_eq!(settings.scheduled_cleanup_minute, 59);
        assert_eq!(settings.log_retention_days, 1);
    }

    #[test]
    fn older_settings_gain_schedule_defaults() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("scheduled_cleanup_enabled");
        object.remove("scheduled_cleanup_day");
        object.remove("scheduled_cleanup_days");
        object.remove("scheduled_cleanup_hour");
        object.remove("scheduled_cleanup_minute");
        object.remove("application_log_cleanup_enabled");
        object.remove("system_journal_cleanup_enabled");
        object.remove("log_retention_days");
        object.remove("language");
        object.remove("last_scheduled_cleanup");
        object.remove("check_for_updates");

        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert!(!settings.scheduled_cleanup_enabled);
        assert_eq!(settings.scheduled_cleanup_day, ScheduleDay::EveryDay);
        assert_eq!(
            settings.effective_scheduled_cleanup_days(),
            ScheduleDay::weekdays()
        );
        assert_eq!(settings.scheduled_cleanup_hour, 3);
        assert_eq!(settings.scheduled_cleanup_minute, 0);
        assert!(!settings.application_log_cleanup_enabled);
        assert!(!settings.system_journal_cleanup_enabled);
        assert_eq!(settings.log_retention_days, 30);
        assert_eq!(settings.language, AppLanguage::Automatic);
        assert_eq!(settings.last_scheduled_cleanup, None);
        assert!(settings.check_for_updates);
    }

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        // October 2026: the 12th is a Monday.
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    fn daily_at_three() -> AppSettings {
        AppSettings {
            scheduled_cleanup_enabled: true,
            scheduled_cleanup_hour: 3,
            scheduled_cleanup_minute: 0,
            ..AppSettings::default()
        }
    }

    #[test]
    fn a_missed_slot_is_caught_up_later() {
        let mut settings = daily_at_three();
        settings.last_scheduled_cleanup = Some(at(12, 3, 0));

        // Asleep at 03:00 on Tuesday, opened at 09:00.
        assert!(settings.scheduled_cleanup_due(&at(13, 9, 0)));
    }

    #[test]
    fn a_slot_that_already_ran_is_not_repeated() {
        let mut settings = daily_at_three();
        settings.last_scheduled_cleanup = Some(at(13, 3, 0) + Duration::seconds(20));

        assert!(!settings.scheduled_cleanup_due(&at(13, 3, 1)));
        assert!(!settings.scheduled_cleanup_due(&at(13, 23, 59)));
        assert!(settings.scheduled_cleanup_due(&at(14, 3, 0)));
    }

    #[test]
    fn several_missed_slots_mean_one_due_run() {
        let mut settings = daily_at_three();
        settings.last_scheduled_cleanup = Some(at(5, 3, 0));

        let now = at(13, 9, 0);
        assert_eq!(settings.latest_scheduled_slot(&now), Some(at(13, 3, 0)));
        assert!(settings.scheduled_cleanup_due(&now));
    }

    #[test]
    fn weekly_schedule_finds_last_weeks_slot() {
        let mut settings = daily_at_three();
        settings.set_scheduled_cleanup_days(vec![ScheduleDay::Wednesday]);

        // Sunday the 18th: the latest Wednesday slot is the 14th.
        let now = at(18, 12, 0);
        assert_eq!(now.weekday(), chrono::Weekday::Sun);
        assert_eq!(settings.latest_scheduled_slot(&now), Some(at(14, 3, 0)));

        settings.last_scheduled_cleanup = Some(at(14, 3, 0));
        assert!(!settings.scheduled_cleanup_due(&now));
        settings.last_scheduled_cleanup = Some(at(7, 3, 0));
        assert!(settings.scheduled_cleanup_due(&now));
    }

    #[test]
    fn nothing_is_due_without_a_baseline_or_when_disabled() {
        let mut settings = daily_at_three();
        assert!(!settings.scheduled_cleanup_due(&at(13, 9, 0)));

        settings.last_scheduled_cleanup = Some(at(1, 0, 0));
        settings.scheduled_cleanup_enabled = false;
        assert!(!settings.scheduled_cleanup_due(&at(13, 9, 0)));
    }

    #[test]
    fn changing_the_schedule_restarts_the_catch_up_window() {
        let mut settings = daily_at_three();
        settings.last_scheduled_cleanup = Some(at(1, 0, 0));
        settings.mark_schedule_changed();

        let now = Utc::now();
        assert!(!settings.scheduled_cleanup_due(&now));
    }

    #[test]
    fn schedule_day_matches_only_the_selected_weekday() {
        assert!(ScheduleDay::EveryDay.matches(chrono::Weekday::Sun));
        assert!(ScheduleDay::Wednesday.matches(chrono::Weekday::Wed));
        assert!(!ScheduleDay::Wednesday.matches(chrono::Weekday::Thu));
    }

    #[test]
    fn multi_day_schedule_matches_each_selected_weekday() {
        let mut settings = AppSettings::default();
        settings.set_scheduled_cleanup_days(vec![
            ScheduleDay::Monday,
            ScheduleDay::Wednesday,
            ScheduleDay::Friday,
        ]);

        assert!(settings.scheduled_cleanup_matches(chrono::Weekday::Mon));
        assert!(settings.scheduled_cleanup_matches(chrono::Weekday::Wed));
        assert!(settings.scheduled_cleanup_matches(chrono::Weekday::Fri));
        assert!(!settings.scheduled_cleanup_matches(chrono::Weekday::Tue));
    }

    #[test]
    fn legacy_single_day_schedule_is_preserved() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.insert(
            "scheduled_cleanup_day".to_string(),
            serde_json::to_value(ScheduleDay::Wednesday).unwrap(),
        );
        object.remove("scheduled_cleanup_days");

        let settings: AppSettings = serde_json::from_value(value).unwrap();
        assert_eq!(
            settings.effective_scheduled_cleanup_days(),
            vec![ScheduleDay::Wednesday]
        );
    }
}
