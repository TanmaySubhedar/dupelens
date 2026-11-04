//! How alike two files must be to count as duplicates, as a setting the user can change.
//!
//! The detection pipeline (decision 0003) has four numbers that decide whether two photos
//! are "the same picture". People differ on what counts as a look-alike, so they are exposed
//! as four presets and, for those who want it, as the raw values (`Custom`). A preset is just a
//! named set of those four values, plus one switch that turns look-alike matching off.
//!
//! Changing a preset never needs a rescan: fingerprints and pixel-comparison scores are cached,
//! so only look-alike pairs not seen before are compared.

use crate::error::{Error, Result};
use crate::grouping::GroupingConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    /// Byte-for-byte identical files only.
    ExactOnly,
    /// Near-identical photos.
    Strict,
    /// Resized or re-saved copies; keeps screenshots that only look alike apart. The default.
    Balanced,
    /// Edits and crops may match. Review carefully.
    Loose,
    /// The four raw values, chosen by the user.
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::ExactOnly,
        Preset::Strict,
        Preset::Balanced,
        Preset::Loose,
        Preset::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Preset::ExactOnly => "exact_only",
            Preset::Strict => "strict",
            Preset::Balanced => "balanced",
            Preset::Loose => "loose",
            Preset::Custom => "custom",
        }
    }

    pub fn from_key(key: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            Preset::ExactOnly => "Exact only",
            Preset::Strict => "Strict",
            Preset::Balanced => "Balanced",
            Preset::Loose => "Loose",
            Preset::Custom => "Custom",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Preset::ExactOnly => "Only files that are byte-for-byte identical",
            Preset::Strict => "Near-identical photos",
            Preset::Balanced => {
                "Also finds photos that were resized or saved again, and keeps screenshots that only look alike apart"
            }
            Preset::Loose => "Edits and crops may match too; review these carefully",
            Preset::Custom => "Your own values",
        }
    }

    /// The values a preset stands for. `Custom` has none of its own: see [`MatchingSettings`].
    pub fn config(self) -> GroupingConfig {
        let balanced = GroupingConfig::default();
        match self {
            Preset::ExactOnly => GroupingConfig {
                look_alikes: false,
                ..balanced
            },
            Preset::Strict => GroupingConfig {
                phash_max: 4,
                dhash_max: 6,
                aspect_tolerance: 0.03,
                max_local_difference: 10.0,
                ..balanced
            },
            Preset::Balanced | Preset::Custom => balanced,
            Preset::Loose => GroupingConfig {
                phash_max: 12,
                dhash_max: 18,
                aspect_tolerance: 0.08,
                max_local_difference: 30.0,
                ..balanced
            },
        }
    }
}

/// The chosen preset and the values that go with it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MatchingSettings {
    pub preset: Preset,
    pub config: GroupingConfig,
}

impl Default for MatchingSettings {
    fn default() -> Self {
        Self::preset(Preset::Balanced)
    }
}

impl MatchingSettings {
    /// A named preset (not `Custom`; use [`MatchingSettings::custom`]).
    pub fn preset(preset: Preset) -> Self {
        Self {
            preset,
            config: preset.config(),
        }
    }

    /// Raw values, checked to be sensible. Look-alike matching is on.
    pub fn custom(config: GroupingConfig) -> Result<Self> {
        let bad = |what: &str| Err(Error::InvalidSetting(what.into()));
        if config.phash_max > 24 {
            return bad("overall-look distance must be between 0 and 24");
        }
        if config.dhash_max > 32 {
            return bad("fine-detail distance must be between 0 and 32");
        }
        if !(0.0..=0.25).contains(&config.aspect_tolerance) {
            return bad("shape tolerance must be between 0% and 25%");
        }
        if !(0.0..=60.0).contains(&config.max_local_difference) {
            return bad("allowed pixel change must be between 0 and 60");
        }
        Ok(Self {
            preset: Preset::Custom,
            config: GroupingConfig {
                look_alikes: true,
                ..config
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_is_todays_behaviour() {
        assert_eq!(Preset::Balanced.config(), GroupingConfig::default());
        assert_eq!(MatchingSettings::default().preset, Preset::Balanced);
    }

    #[test]
    fn presets_get_looser_in_every_dimension() {
        let (s, b, l) = (
            Preset::Strict.config(),
            Preset::Balanced.config(),
            Preset::Loose.config(),
        );
        assert!(s.phash_max < b.phash_max && b.phash_max < l.phash_max);
        assert!(s.dhash_max < b.dhash_max && b.dhash_max < l.dhash_max);
        assert!(s.aspect_tolerance < b.aspect_tolerance && b.aspect_tolerance < l.aspect_tolerance);
        assert!(s.max_local_difference < b.max_local_difference);
        assert!(b.max_local_difference < l.max_local_difference);
    }

    #[test]
    fn only_exact_only_turns_look_alikes_off() {
        for p in Preset::ALL {
            assert_eq!(p.config().look_alikes, p != Preset::ExactOnly, "{p:?}");
        }
    }

    #[test]
    fn keys_round_trip() {
        for p in Preset::ALL {
            assert_eq!(Preset::from_key(p.key()), Some(p));
        }
        assert_eq!(Preset::from_key("nonsense"), None);
    }

    #[test]
    fn custom_values_are_validated() {
        let ok = GroupingConfig {
            phash_max: 6,
            ..GroupingConfig::default()
        };
        let s = MatchingSettings::custom(ok).unwrap();
        assert_eq!((s.preset, s.config.phash_max), (Preset::Custom, 6));
        for bad in [
            GroupingConfig {
                phash_max: 25,
                ..ok
            },
            GroupingConfig {
                dhash_max: 33,
                ..ok
            },
            GroupingConfig {
                aspect_tolerance: 0.3,
                ..ok
            },
            GroupingConfig {
                aspect_tolerance: -0.1,
                ..ok
            },
            GroupingConfig {
                max_local_difference: 61.0,
                ..ok
            },
        ] {
            assert!(matches!(
                MatchingSettings::custom(bad),
                Err(Error::InvalidSetting(_))
            ));
        }
    }
}
