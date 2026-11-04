//! Automation rules: model and window matching.
//!
//! A rule matches a window by executable (file name or full path, optional `*`/`?`
//! wildcards) and an optional title matcher. Matching is case-insensitive unless the title
//! matcher requests case sensitivity. The first enabled matching rule wins.

use serde::{Deserialize, Serialize};

/// How a title pattern is compared with a window title.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleMatchMode {
    #[default]
    Contains,
    Equals,
    StartsWith,
    EndsWith,
    /// Glob-style pattern where `*` matches any run of characters and `?` one character.
    Wildcard,
}

/// Optional window-title condition of a rule.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TitleMatcher {
    pub mode: TitleMatchMode,
    pub pattern: String,
    pub case_sensitive: bool,
}

impl TitleMatcher {
    /// Whether `title` satisfies this matcher. An empty pattern matches every title.
    #[must_use]
    pub fn matches(&self, title: &str) -> bool {
        if self.pattern.is_empty() {
            return true;
        }
        let (pattern, title) = if self.case_sensitive {
            (self.pattern.clone(), title.to_owned())
        } else {
            (self.pattern.to_lowercase(), title.to_lowercase())
        };
        match self.mode {
            TitleMatchMode::Contains => title.contains(&pattern),
            TitleMatchMode::Equals => title == pattern,
            TitleMatchMode::StartsWith => title.starts_with(&pattern),
            TitleMatchMode::EndsWith => title.ends_with(&pattern),
            TitleMatchMode::Wildcard => wildcard_match(&pattern, &title),
        }
    }
}

/// An automatic pinning/transparency rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Rule {
    /// Stable unique identifier.
    pub id: String,
    /// Display name shown in the rule manager.
    pub name: String,
    pub enabled: bool,
    /// Executable file name (`notepad.exe`), full path (`C:\Tools\app.exe`) or a wildcard
    /// pattern of either. A name without extension also matches `<name>.exe`.
    pub exe: String,
    /// Optional title condition.
    pub title: Option<TitleMatcher>,
    /// Pin matching windows (always on top).
    pub pin: bool,
    /// Opacity in percent applied to matching windows (100 = opaque).
    pub opacity: u8,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            exe: String::new(),
            title: None,
            pin: true,
            opacity: 100,
        }
    }
}

/// The facts about a window needed for rule matching.
#[derive(Clone, Copy, Debug)]
pub struct WindowFacts<'a> {
    /// Full image path of the (effective) owning process, e.g. `C:\Windows\notepad.exe`.
    pub exe_path: &'a str,
    /// Window title.
    pub title: &'a str,
}

impl Rule {
    /// Whether the rule has an effect at all (it pins or changes opacity).
    #[must_use]
    pub const fn has_effect(&self) -> bool {
        self.pin || self.opacity < 100
    }

    /// Whether this rule matches the window. Disabled rules never match.
    #[must_use]
    pub fn matches(&self, window: &WindowFacts<'_>) -> bool {
        self.enabled
            && self.has_effect()
            && exe_matches(&self.exe, window.exe_path)
            && self.title.as_ref().is_none_or(|t| t.matches(window.title))
    }
}

/// Returns the first enabled rule matching the window.
#[must_use]
pub fn find_matching_rule<'a>(rules: &'a [Rule], window: &WindowFacts<'_>) -> Option<&'a Rule> {
    rules.iter().find(|r| r.matches(window))
}

/// Whether any enabled rule uses a title condition (the engine only listens to title
/// changes when this is true).
#[must_use]
pub fn any_title_rules(rules: &[Rule]) -> bool {
    rules
        .iter()
        .any(|r| r.enabled && r.title.as_ref().is_some_and(|t| !t.pattern.is_empty()))
}

/// Extracts the file name from a Windows or POSIX path.
#[must_use]
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Normalises an executable pattern typed by a user: trims whitespace and quotes.
#[must_use]
pub fn normalize_exe_pattern(pattern: &str) -> String {
    pattern.trim().trim_matches('"').trim().to_owned()
}

/// Case-insensitive executable match. Patterns containing a path separator are compared
/// with the full path, other patterns with the file name only.
#[must_use]
pub fn exe_matches(pattern: &str, exe_path: &str) -> bool {
    let pattern = normalize_exe_pattern(pattern);
    if pattern.is_empty() || exe_path.is_empty() {
        return false;
    }
    let pattern = pattern.replace('/', "\\").to_lowercase();
    let path = exe_path.replace('/', "\\").to_lowercase();
    if pattern.contains('\\') {
        return wildcard_match(&pattern, &path);
    }
    let name = file_name(&path);
    if wildcard_match(&pattern, name) {
        return true;
    }
    // "notepad" also matches "notepad.exe".
    let has_extension = file_name(&pattern).contains('.');
    !has_extension
        && name
            .strip_suffix(".exe")
            .is_some_and(|stem| wildcard_match(&pattern, stem))
}

/// Glob match supporting `*` (any run, including empty) and `?` (exactly one character).
/// Runs in `O(pattern × text)` worst case without recursion.
#[must_use]
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_ti = 0usize;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(s) = star {
            pi = s + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts<'a>(exe_path: &'a str, title: &'a str) -> WindowFacts<'a> {
        WindowFacts { exe_path, title }
    }

    fn rule(exe: &str) -> Rule {
        Rule {
            id: "r".into(),
            name: "r".into(),
            exe: exe.into(),
            ..Rule::default()
        }
    }

    #[test]
    fn wildcard_basics() {
        assert!(wildcard_match("*", ""));
        assert!(wildcard_match("a*c", "abbbc"));
        assert!(wildcard_match("a?c", "abc"));
        assert!(!wildcard_match("a?c", "ac"));
        assert!(wildcard_match("*.exe", "notepad.exe"));
        assert!(!wildcard_match("*.exe", "notepad.dll"));
        assert!(wildcard_match("**x", "x"));
        assert!(!wildcard_match("", "x"));
        assert!(wildcard_match("", ""));
        assert!(wildcard_match("*mid*", "a mid b"));
    }

    #[test]
    fn exe_matching_by_name_is_case_insensitive() {
        let p = r"C:\Windows\System32\NOTEPAD.EXE";
        assert!(exe_matches("notepad.exe", p));
        assert!(exe_matches("Notepad.exe", p));
        assert!(exe_matches("notepad", p));
        assert!(exe_matches(" \"notepad.exe\" ", p));
        assert!(!exe_matches("note", p));
        assert!(!exe_matches("notepad.com", p));
        assert!(!exe_matches("", p));
        assert!(!exe_matches("notepad.exe", ""));
    }

    #[test]
    fn exe_matching_by_path_and_wildcards() {
        let p = r"C:\Program Files\Discord\app-1.0\Discord.exe";
        assert!(exe_matches(r"C:\Program Files\Discord\*\Discord.exe", p));
        assert!(exe_matches(
            "c:/program files/discord/app-1.0/discord.exe",
            p
        ));
        assert!(!exe_matches(r"D:\Discord.exe", p));
        assert!(exe_matches("disc*", p));
    }

    #[test]
    fn title_matchers() {
        let m = |mode, pattern: &str| TitleMatcher {
            mode,
            pattern: pattern.into(),
            case_sensitive: false,
        };
        assert!(m(TitleMatchMode::Contains, "notes").matches("My Notes - Editor"));
        assert!(m(TitleMatchMode::Equals, "calculator").matches("Calculator"));
        assert!(!m(TitleMatchMode::Equals, "calc").matches("Calculator"));
        assert!(m(TitleMatchMode::StartsWith, "my").matches("My Notes"));
        assert!(m(TitleMatchMode::EndsWith, "editor").matches("My Notes - Editor"));
        assert!(m(TitleMatchMode::Wildcard, "* - discord").matches("#general - Discord"));
        assert!(m(TitleMatchMode::Contains, "").matches("anything"));
        let cs = TitleMatcher {
            mode: TitleMatchMode::Contains,
            pattern: "Notes".into(),
            case_sensitive: true,
        };
        assert!(!cs.matches("my notes"));
        assert!(cs.matches("my Notes"));
    }

    #[test]
    fn first_enabled_matching_rule_wins() {
        let mut disabled = rule("notepad.exe");
        disabled.enabled = false;
        disabled.id = "disabled".into();
        let mut titled = rule("notepad.exe");
        titled.id = "titled".into();
        titled.title = Some(TitleMatcher {
            mode: TitleMatchMode::Contains,
            pattern: "todo".into(),
            case_sensitive: false,
        });
        let mut generic = rule("notepad.exe");
        generic.id = "generic".into();
        let rules = vec![disabled, titled, generic];
        let exe = r"C:\Windows\notepad.exe";
        assert_eq!(
            find_matching_rule(&rules, &facts(exe, "todo.txt")).map(|r| r.id.as_str()),
            Some("titled")
        );
        assert_eq!(
            find_matching_rule(&rules, &facts(exe, "other.txt")).map(|r| r.id.as_str()),
            Some("generic")
        );
        assert!(find_matching_rule(&rules, &facts(r"C:\calc.exe", "x")).is_none());
    }

    #[test]
    fn rules_without_effect_never_match() {
        let mut r = rule("notepad.exe");
        r.pin = false;
        r.opacity = 100;
        assert!(!r.matches(&facts(r"C:\notepad.exe", "")));
        r.opacity = 80;
        assert!(r.matches(&facts(r"C:\notepad.exe", "")));
    }

    #[test]
    fn title_rule_detection() {
        let mut r = rule("a.exe");
        assert!(!any_title_rules(std::slice::from_ref(&r)));
        r.title = Some(TitleMatcher {
            pattern: "x".into(),
            ..TitleMatcher::default()
        });
        assert!(any_title_rules(std::slice::from_ref(&r)));
        r.enabled = false;
        assert!(!any_title_rules(&[r]));
    }

    #[test]
    fn file_name_extraction() {
        assert_eq!(file_name(r"C:\a\b.exe"), "b.exe");
        assert_eq!(file_name("b.exe"), "b.exe");
        assert_eq!(file_name("/x/y"), "y");
    }
}
