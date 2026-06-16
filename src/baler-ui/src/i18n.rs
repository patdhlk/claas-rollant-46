// SPDX-License-Identifier: GPL-3.0-only
//! Internationalisation string table for the baler panel UI (ISSUE_0005).
//!
//! This module holds every static UI label in English and German in one place
//! per language — the single source of truth for the UI's text. The Slint view
//! and the language-aware mode/knife/fault mappers are wired to it (ISSUE_0006);
//! the runtime toggle between EN and DE is handled by F3 on the service screen
//! (ISSUE_0007). It is pure Rust — no Slint, no filesystem access — and therefore
//! compiles and tests on the host without the `device` feature.

// ---------------------------------------------------------------------------
// Language discriminant
// ---------------------------------------------------------------------------

/// The two display languages supported by the baler UI.
///
/// The product default is German ([`Lang::De`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    En,
    #[default]
    De,
}

impl Lang {
    /// Return the ISO 639-1 language code for this language.
    ///
    /// ```text
    /// Lang::En.as_code() == "en"
    /// Lang::De.as_code() == "de"
    /// ```
    pub fn as_code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    /// Parse a language code, falling back to the default ([`Lang::De`]) for
    /// any unrecognised code.
    ///
    /// ```text
    /// Lang::from_code("en") == Lang::En
    /// Lang::from_code("de") == Lang::De
    /// Lang::from_code("fr") == Lang::De   // unknown → default
    /// Lang::from_code("")   == Lang::De   // empty   → default
    /// ```
    pub fn from_code(code: &str) -> Lang {
        match code {
            "en" => Lang::En,
            "de" => Lang::De,
            _ => Lang::default(),
        }
    }

    /// Return the other language (En↔De toggle).
    ///
    /// ```text
    /// Lang::En.other() == Lang::De
    /// Lang::De.other() == Lang::En
    /// ```
    pub fn other(self) -> Lang {
        match self {
            Lang::En => Lang::De,
            Lang::De => Lang::En,
        }
    }
}

// ---------------------------------------------------------------------------
// String table
// ---------------------------------------------------------------------------

/// All static UI labels for one language.
///
/// Every field is a `&'static str` so the table lives entirely in ROM.
/// Access via [`table`] rather than constructing directly.
pub struct Strings {
    // Main screen
    pub title: &'static str,
    pub language_name: &'static str,
    pub bale_full_banner: &'static str,
    pub session_caption: &'static str,
    pub total_caption: &'static str,
    pub knife_caption: &'static str,
    pub sk_wrap: &'static str,
    pub sk_wrapping: &'static str,
    pub sk_knife_toggle: &'static str,
    pub sk_knife_active: &'static str,
    pub sk_reset_session: &'static str,
    pub sk_service: &'static str,

    // Service screen
    pub service_title: &'static str,
    pub enter_pin: &'static str,
    pub pin_hint: &'static str,
    pub network_caption: &'static str,
    pub sk_reset_total: &'static str,
    pub sk_use_ethernet: &'static str,
    pub sk_use_ethercat: &'static str,
    pub sk_io_test: &'static str,
    pub sk_back: &'static str,

    // Ethernet screen
    pub eth_title: &'static str,
    pub eth_offline: &'static str,
    pub static_ip_caption: &'static str,
    pub sk_return_ethercat: &'static str,

    // Fault screen
    pub fault_title: &'static str,
    pub fault_detail: &'static str,
    pub fault_hint: &'static str,

    // State-derived texts (single source of truth for language-aware mappers)
    pub mode_initialising: &'static str,
    pub mode_operational: &'static str,
    pub mode_fault: &'static str,
    pub mode_ethernet: &'static str,
    pub knife_unknown: &'static str,
    pub knife_in: &'static str,
    pub knife_out: &'static str,
    pub fault_link_lost: &'static str,
}

/// English string table.
pub const EN: Strings = Strings {
    // Main screen
    title: "BALER",
    language_name: "ENGLISH",
    bale_full_banner: "● BALE FULL — READY TO WRAP",
    session_caption: "SESSION",
    total_caption: "TOTAL",
    knife_caption: "KNIFE",
    sk_wrap: "WRAP",
    sk_wrapping: "WRAPPING…",
    sk_knife_toggle: "TOGGLE KNIVES",
    sk_knife_active: "KNIVES…",
    sk_reset_session: "RESET SESSION",
    sk_service: "SERVICE",

    // Service screen
    service_title: "SERVICE",
    enter_pin: "ENTER PIN",
    pin_hint: "▲▼ digit   ◀▶ position   Enter confirm",
    network_caption: "NETWORK",
    sk_reset_total: "RESET TOTAL",
    sk_use_ethernet: "USE ETHERNET",
    sk_use_ethercat: "USE ETHERCAT",
    sk_io_test: "IO TEST",
    sk_back: "BACK",

    // Ethernet screen
    eth_title: "ETHERNET MODE",
    eth_offline: "CONTROL OFFLINE — ETHERCAT STOPPED",
    static_ip_caption: "STATIC IP",
    sk_return_ethercat: "RETURN TO ETHERCAT",

    // Fault screen
    fault_title: "⚠ FAULT",
    fault_detail: "Outputs dropped by the coupler watchdog.\nClears automatically when the bus recovers.",
    fault_hint: "F6 → SERVICE  ·  Ethernet maintenance mode",

    // State-derived texts
    mode_initialising: "INITIALISING",
    mode_operational: "OPERATIONAL",
    mode_fault: "FAULT",
    mode_ethernet: "ETHERNET",
    knife_unknown: "UNKNOWN",
    knife_in: "IN",
    knife_out: "OUT",
    fault_link_lost: "ETHERCAT LINK LOST",
};

/// German string table.
pub const DE: Strings = Strings {
    // Main screen
    title: "BALER",
    language_name: "DEUTSCH",
    bale_full_banner: "● BALLEN VOLL — BEREIT ZUM WICKELN",
    session_caption: "SCHICHT",
    total_caption: "GESAMT",
    knife_caption: "MESSER",
    sk_wrap: "WICKELN",
    sk_wrapping: "WICKELT…",
    sk_knife_toggle: "MESSER SCHALTEN",
    sk_knife_active: "MESSER…",
    sk_reset_session: "SCHICHT NULLEN",
    sk_service: "WARTUNG",

    // Service screen
    service_title: "WARTUNG",
    enter_pin: "PIN EINGEBEN",
    pin_hint: "▲▼ Ziffer   ◀▶ Position   Enter Bestätigen",
    network_caption: "NETZWERK",
    sk_reset_total: "GESAMT NULLEN",
    sk_use_ethernet: "ETHERNET NUTZEN",
    sk_use_ethercat: "ETHERCAT NUTZEN",
    sk_io_test: "EA-TEST",
    sk_back: "ZURÜCK",

    // Ethernet screen
    eth_title: "ETHERNET-MODUS",
    eth_offline: "STEUERUNG OFFLINE — ETHERCAT GESTOPPT",
    static_ip_caption: "STATISCHE IP",
    sk_return_ethercat: "ZURÜCK ZU ETHERCAT",

    // Fault screen
    fault_title: "⚠ STÖRUNG",
    fault_detail: "Ausgänge durch Koppler-Watchdog abgeschaltet.\nWird automatisch zurückgesetzt, sobald der Bus wieder verfügbar ist.",
    fault_hint: "F6 → WARTUNG  ·  Ethernet-Wartungsmodus",

    // State-derived texts
    mode_initialising: "INITIALISIERUNG",
    mode_operational: "BETRIEB",
    mode_fault: "STÖRUNG",
    mode_ethernet: "ETHERNET",
    knife_unknown: "UNBEKANNT",
    knife_in: "INNEN",
    knife_out: "AUSSEN",
    fault_link_lost: "ETHERCAT-VERBINDUNG VERLOREN",
};

/// Return a reference to the compile-time string table for `lang`.
pub fn table(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::De => &DE,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Lang code round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn lang_code_round_trips_en() {
        assert_eq!(Lang::from_code(Lang::En.as_code()), Lang::En);
    }

    #[test]
    fn lang_code_round_trips_de() {
        assert_eq!(Lang::from_code(Lang::De.as_code()), Lang::De);
    }

    #[test]
    fn lang_codes_are_correct() {
        assert_eq!(Lang::En.as_code(), "en");
        assert_eq!(Lang::De.as_code(), "de");
    }

    #[test]
    fn unknown_code_falls_back_to_default_de() {
        for unknown in ["fr", "es", "zh", "EN", "DE", "", " ", "english"] {
            assert_eq!(
                Lang::from_code(unknown),
                Lang::De,
                "expected De for unknown code {unknown:?}"
            );
        }
    }

    #[test]
    fn lang_default_is_de() {
        assert_eq!(Lang::default(), Lang::De);
    }

    #[test]
    fn lang_other_en_gives_de() {
        assert_eq!(Lang::En.other(), Lang::De);
    }

    #[test]
    fn lang_other_de_gives_en() {
        assert_eq!(Lang::De.other(), Lang::En);
    }

    #[test]
    fn lang_other_is_involution() {
        assert_eq!(Lang::En.other().other(), Lang::En);
        assert_eq!(Lang::De.other().other(), Lang::De);
    }

    // -----------------------------------------------------------------------
    // String table completeness
    //
    // Strategy: an accessor-closure slice drives the non-empty completeness
    // check. The guard that matters most is that `Strings` uses named-field
    // initialisation without `..default()`, so a newly-added field MUST appear
    // in both `EN` and `DE` or the crate will not compile at all.
    //
    // A newly-added field will not be caught by the ACCESSORS slice specifically
    // (it would still need adding here and to `EXPECTED_FIELD_COUNT` by hand),
    // but the exhaustive struct literal already guarantees it exists in both
    // tables. The `accessor_count_matches_expectation` test then catches the
    // slice drifting out of sync, prompting the author to add the new accessor.
    // -----------------------------------------------------------------------

    type Accessor = fn(&Strings) -> &'static str;

    const ACCESSORS: &[(&str, Accessor)] = &[
        // Main screen
        ("title", |s| s.title),
        ("language_name", |s| s.language_name),
        ("bale_full_banner", |s| s.bale_full_banner),
        ("session_caption", |s| s.session_caption),
        ("total_caption", |s| s.total_caption),
        ("knife_caption", |s| s.knife_caption),
        ("sk_wrap", |s| s.sk_wrap),
        ("sk_wrapping", |s| s.sk_wrapping),
        ("sk_knife_toggle", |s| s.sk_knife_toggle),
        ("sk_knife_active", |s| s.sk_knife_active),
        ("sk_reset_session", |s| s.sk_reset_session),
        ("sk_service", |s| s.sk_service),
        // Service screen
        ("service_title", |s| s.service_title),
        ("enter_pin", |s| s.enter_pin),
        ("pin_hint", |s| s.pin_hint),
        ("network_caption", |s| s.network_caption),
        ("sk_reset_total", |s| s.sk_reset_total),
        ("sk_use_ethernet", |s| s.sk_use_ethernet),
        ("sk_use_ethercat", |s| s.sk_use_ethercat),
        ("sk_io_test", |s| s.sk_io_test),
        ("sk_back", |s| s.sk_back),
        // Ethernet screen
        ("eth_title", |s| s.eth_title),
        ("eth_offline", |s| s.eth_offline),
        ("static_ip_caption", |s| s.static_ip_caption),
        ("sk_return_ethercat", |s| s.sk_return_ethercat),
        // Fault screen
        ("fault_title", |s| s.fault_title),
        ("fault_detail", |s| s.fault_detail),
        ("fault_hint", |s| s.fault_hint),
        // State-derived
        ("mode_initialising", |s| s.mode_initialising),
        ("mode_operational", |s| s.mode_operational),
        ("mode_fault", |s| s.mode_fault),
        ("mode_ethernet", |s| s.mode_ethernet),
        ("knife_unknown", |s| s.knife_unknown),
        ("knife_in", |s| s.knife_in),
        ("knife_out", |s| s.knife_out),
        ("fault_link_lost", |s| s.fault_link_lost),
    ];

    #[test]
    fn every_field_non_empty_in_english() {
        let s = table(Lang::En);
        for (name, f) in ACCESSORS {
            let v = f(s);
            assert!(!v.is_empty(), "EN.{name} is empty");
        }
    }

    #[test]
    fn every_field_non_empty_in_german() {
        let s = table(Lang::De);
        for (name, f) in ACCESSORS {
            let v = f(s);
            assert!(!v.is_empty(), "DE.{name} is empty");
        }
    }

    #[test]
    fn en_and_de_differ_except_for_intentionally_shared_fields() {
        // A real translation must differ between languages. The only fields that
        // are legitimately identical in EN and DE are the brand title and the
        // protocol name "ETHERNET". Anything else matching means an English
        // string was copy-pasted into the German table (or vice-versa).
        const SHARED: &[&str] = &["title", "mode_ethernet"];
        let (en, de) = (table(Lang::En), table(Lang::De));
        for (name, f) in ACCESSORS {
            if SHARED.contains(name) {
                continue;
            }
            assert_ne!(
                f(en),
                f(de),
                "EN.{name} and DE.{name} are identical (\"{}\") — likely an \
                 untranslated string. Add it to SHARED only if that is intended.",
                f(en)
            );
        }
    }

    // -----------------------------------------------------------------------
    // Spot-check selected strings for correct content
    // -----------------------------------------------------------------------

    #[test]
    fn spot_check_en_strings() {
        let s = table(Lang::En);
        assert_eq!(s.title, "BALER");
        assert_eq!(s.language_name, "ENGLISH");
        assert_eq!(s.bale_full_banner, "● BALE FULL — READY TO WRAP");
        assert_eq!(s.pin_hint, "▲▼ digit   ◀▶ position   Enter confirm");
        assert_eq!(s.fault_title, "⚠ FAULT");
        assert!(s.fault_detail.contains('\n'), "fault_detail must contain a newline");
        assert_eq!(s.mode_initialising, "INITIALISING");
        assert_eq!(s.knife_in, "IN");
        assert_eq!(s.knife_out, "OUT");
        assert_eq!(s.fault_link_lost, "ETHERCAT LINK LOST");
    }

    #[test]
    fn spot_check_de_strings() {
        let s = table(Lang::De);
        assert_eq!(s.title, "BALER");
        assert_eq!(s.language_name, "DEUTSCH");
        assert_eq!(s.bale_full_banner, "● BALLEN VOLL — BEREIT ZUM WICKELN");
        assert_eq!(s.pin_hint, "▲▼ Ziffer   ◀▶ Position   Enter Bestätigen");
        assert_eq!(s.fault_title, "⚠ STÖRUNG");
        assert!(s.fault_detail.contains('\n'), "fault_detail must contain a newline");
        assert_eq!(s.mode_initialising, "INITIALISIERUNG");
        assert_eq!(s.knife_in, "INNEN");
        assert_eq!(s.knife_out, "AUSSEN");
        assert_eq!(s.fault_link_lost, "ETHERCAT-VERBINDUNG VERLOREN");
    }

    #[test]
    fn table_fn_returns_correct_table() {
        // Verify that table() routes to the right language by checking a
        // language-discriminating field.  Pointer identity is not reliable
        // for `const` statics (the compiler may copy them), so compare values.
        assert_eq!(table(Lang::En).language_name, EN.language_name);
        assert_eq!(table(Lang::De).language_name, DE.language_name);
        // The two tables must differ on at least one field.
        assert_ne!(
            table(Lang::En).language_name,
            table(Lang::De).language_name
        );
    }

    // -----------------------------------------------------------------------
    // Field count guard
    //
    // This count must equal the number of fields in `Strings`.  If you add a
    // field to `Strings` you MUST also add an entry to ACCESSORS and update
    // this constant — the test will fail loudly until you do.
    // -----------------------------------------------------------------------
    #[test]
    fn accessor_count_matches_expectation() {
        // Update this when new fields are added to Strings AND ACCESSORS.
        const EXPECTED_FIELD_COUNT: usize = 36;
        assert_eq!(
            ACCESSORS.len(),
            EXPECTED_FIELD_COUNT,
            "ACCESSORS slice has {} entries but expected {}. \
             Did you add a field to Strings without updating ACCESSORS?",
            ACCESSORS.len(),
            EXPECTED_FIELD_COUNT
        );
    }
}
