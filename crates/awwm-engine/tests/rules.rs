//! Automation rules against real windows of `awwm-testapp.exe`.
//!
//! Rules pin a window as soon as it is shown. Scenarios where the engine pins a *fresh*
//! window run inside [`retry_if_topmost_refused`] (see `common` for why).
#![cfg(windows)]

mod common;

use std::time::Duration;

use awwm_core::rules::{Rule, TitleMatchMode, TitleMatcher};
use awwm_core::settings::{Settings, opacity_to_alpha};
use awwm_engine::ManagedSource;
use common::{
    QUIET_PERIOD, TestApp, TestEngine, alpha, is_layered, is_topmost, retry_if_topmost_refused,
    settings, setup, spot, testapp_rule,
};

fn title_contains(pattern: &str) -> Option<TitleMatcher> {
    Some(TitleMatcher {
        mode: TitleMatchMode::Contains,
        pattern: pattern.to_owned(),
        case_sensitive: false,
    })
}

fn with_rules(rules: Vec<Rule>) -> Settings {
    let mut s = settings();
    s.rules = rules;
    s
}

#[test]
fn rule_pins_and_sets_opacity_on_a_new_window() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let engine = TestEngine::start(with_rules(vec![testapp_rule("r-new", true, 80)]));

        let app = TestApp::spawn_at("AWWM.Test.RuleNew", spot(0), &[]);
        let h = app.hwnd(0);

        let info = engine.wait_pinned_by_engine(h)?;
        assert!(info.pinned);
        assert_eq!(info.opacity, 80);
        assert_eq!(info.source, ManagedSource::Rule);
        assert_eq!(info.rule_id.as_deref(), Some("r-new"));
        assert!(is_topmost(h));
        assert_eq!(alpha(h), Some(opacity_to_alpha(80)));
        Ok(())
    });
}

#[test]
fn opacity_only_rule_makes_a_new_window_transparent_without_pinning() {
    let _guard = setup();
    let engine = TestEngine::start(with_rules(vec![testapp_rule("r-glass", false, 65)]));

    let app = TestApp::spawn_at("AWWM.Test.RuleGlass", spot(0), &[]);
    let h = app.hwnd(0);

    let info = engine.wait_managed(h);
    assert!(!info.pinned);
    assert_eq!(info.opacity, 65);
    assert!(!is_topmost(h));
    assert_eq!(alpha(h), Some(opacity_to_alpha(65)));
}

#[test]
fn rule_with_title_condition_only_matches_windows_with_that_title() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let mut rule = testapp_rule("r-title", true, 100);
        rule.title = title_contains("Pin Target");
        let engine = TestEngine::start(with_rules(vec![rule]));

        let matching = TestApp::spawn_at(
            "AWWM.Test.RuleTitle",
            spot(0),
            &["--title", "My Pin Target window"],
        );
        let other = TestApp::spawn_at(
            "AWWM.Test.RuleTitle",
            spot(1),
            &["--title", "Something else"],
        );

        let info = engine.wait_pinned_by_engine(matching.hwnd(0))?;
        assert_eq!(info.rule_id.as_deref(), Some("r-title"));
        std::thread::sleep(QUIET_PERIOD);
        assert!(
            engine.managed(other.hwnd(0)).is_none(),
            "the title does not match"
        );
        assert!(!is_topmost(other.hwnd(0)));
        Ok(())
    });
}

#[test]
fn startup_rules_apply_to_an_existing_window() {
    let _guard = setup();
    let app = TestApp::spawn_pinnable("AWWM.Test.RuleStartup", spot(0), &[]);
    let h = app.hwnd(0);
    let mut s = with_rules(vec![testapp_rule("r-startup", true, 70)]);
    s.general.apply_rules_on_startup = true;

    let engine = TestEngine::start(s);

    let info = engine
        .managed(h)
        .expect("applied before Engine::start returns");
    assert_eq!(info.source, ManagedSource::Rule);
    assert!(is_topmost(h));
    assert_eq!(alpha(h), Some(opacity_to_alpha(70)));
}

#[test]
fn existing_window_is_left_alone_without_startup_rules_until_apply_rules_now() {
    let _guard = setup();
    let app = TestApp::spawn_pinnable("AWWM.Test.RuleNoStartup", spot(0), &[]);
    let h = app.hwnd(0);
    let mut s = with_rules(vec![testapp_rule("r-manual", true, 100)]);
    s.general.apply_rules_on_startup = false;
    let engine = TestEngine::start(s);
    std::thread::sleep(QUIET_PERIOD);
    assert!(engine.managed(h).is_none());
    assert!(!is_topmost(h));

    let changed = engine.handle().apply_rules_now().expect("apply rules now");

    assert_eq!(changed, 1);
    assert!(
        engine
            .managed(h)
            .is_some_and(|m| m.source == ManagedSource::Rule)
    );
    assert!(is_topmost(h));
}

#[test]
fn rule_applies_to_every_window_of_the_application() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let engine = TestEngine::start(with_rules(vec![testapp_rule("r-many", true, 90)]));

        let app = TestApp::spawn_at("AWWM.Test.RuleMany", spot(0), &["--count", "3"]);

        for i in 0..3 {
            let h = app.hwnd(i);
            let info = engine.wait_pinned_by_engine(h)?;
            assert_eq!(info.rule_id.as_deref(), Some("r-many"), "window {i}");
            assert!(is_topmost(h), "window {i}");
            assert_eq!(alpha(h), Some(opacity_to_alpha(90)), "window {i}");
        }
        Ok(())
    });
}

#[test]
fn unpinned_rule_window_is_not_repinned_when_its_title_changes() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let mut rule = testapp_rule("r-once", true, 100);
        rule.title = title_contains("Once");
        let engine = TestEngine::start(with_rules(vec![rule]));
        let mut app = TestApp::spawn_at("AWWM.Test.RuleOnce", spot(0), &["--title", "Once 1"]);
        let h = app.hwnd(0);
        engine.wait_pinned_by_engine(h)?;
        engine.handle().unpin(h).expect("user unpins");

        app.set_title(0, "Once 2");

        std::thread::sleep(QUIET_PERIOD);
        assert!(
            engine.managed(h).is_none(),
            "a rule is applied to a window only once"
        );
        assert!(!is_topmost(h));
        Ok(())
    });
}

#[test]
fn apply_rules_now_reapplies_to_an_unpinned_rule_window() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let engine = TestEngine::start(with_rules(vec![testapp_rule("r-again", true, 100)]));
        let app = TestApp::spawn_at("AWWM.Test.RuleAgain", spot(0), &[]);
        let h = app.hwnd(0);
        engine.wait_pinned_by_engine(h)?;
        engine.handle().unpin(h).expect("user unpins");
        assert!(!is_topmost(h));

        let changed = engine.handle().apply_rules_now().expect("apply rules now");

        assert_eq!(changed, 1);
        assert!(
            engine
                .managed(h)
                .is_some_and(|m| m.source == ManagedSource::Rule)
        );
        assert!(is_topmost(h));
        Ok(())
    });
}

#[test]
fn title_rule_applies_when_the_title_changes_to_a_matching_value() {
    let _guard = setup();
    let mut rule = testapp_rule("r-late", true, 60);
    rule.title = title_contains("Late Match");
    let engine = TestEngine::start(with_rules(vec![rule]));
    let mut app = TestApp::spawn_pinnable("AWWM.Test.RuleLate", spot(0), &["--title", "Initial"]);
    let h = app.hwnd(0);
    std::thread::sleep(QUIET_PERIOD);
    assert!(engine.managed(h).is_none());

    app.set_title(0, "Now a Late Match");

    let info = engine.wait_managed(h);
    assert_eq!(info.rule_id.as_deref(), Some("r-late"));
    assert!(is_topmost(h));
    assert_eq!(alpha(h), Some(opacity_to_alpha(60)));
}

#[test]
fn disabled_rule_does_nothing() {
    let _guard = setup();
    let mut rule = testapp_rule("r-off", true, 50);
    rule.enabled = false;
    let engine = TestEngine::start(with_rules(vec![rule]));

    let app = TestApp::spawn_at("AWWM.Test.RuleOff", spot(0), &[]);
    let h = app.hwnd(0);

    std::thread::sleep(QUIET_PERIOD);
    assert!(engine.managed(h).is_none());
    assert_eq!(
        engine.handle().apply_rules_now().expect("apply rules now"),
        0
    );
    assert!(!is_topmost(h));
    assert!(!is_layered(h));
}

#[test]
fn editing_a_rule_updates_the_windows_it_manages() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let mut s = with_rules(vec![testapp_rule("r-edit", true, 80)]);
        let engine = TestEngine::start(s.clone());
        let app = TestApp::spawn_at("AWWM.Test.RuleEdit", spot(0), &["--count", "2"]);
        for i in 0..2 {
            engine.wait_pinned_by_engine(app.hwnd(i))?;
        }

        s.rules[0].opacity = 50;
        engine.apply_settings(s);

        for i in 0..2 {
            let h = app.hwnd(i);
            assert_eq!(engine.managed(h).map(|m| m.opacity), Some(50), "window {i}");
            assert_eq!(alpha(h), Some(opacity_to_alpha(50)), "window {i}");
            assert!(is_topmost(h), "window {i}");
        }
        Ok(())
    });
}

#[test]
fn editing_a_rule_to_stop_pinning_unpins_its_windows() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let mut s = with_rules(vec![testapp_rule("r-unpin", true, 70)]);
        let engine = TestEngine::start(s.clone());
        let app = TestApp::spawn_at("AWWM.Test.RuleEditPin", spot(0), &[]);
        let h = app.hwnd(0);
        engine.wait_pinned_by_engine(h)?;

        s.rules[0].pin = false;
        engine.apply_settings(s);

        assert!(!is_topmost(h));
        assert!(
            engine
                .managed(h)
                .is_some_and(|m| !m.pinned && m.opacity == 70)
        );
        assert_eq!(alpha(h), Some(opacity_to_alpha(70)));
        Ok(())
    });
}

#[test]
fn first_matching_rule_wins() {
    let _guard = setup();
    retry_if_topmost_refused(|| {
        let mut specific = testapp_rule("r-specific", true, 40);
        specific.title = title_contains("Special");
        let generic = testapp_rule("r-generic", true, 90);
        let engine = TestEngine::start(with_rules(vec![specific, generic]));

        let special =
            TestApp::spawn_at("AWWM.Test.RuleOrder", spot(0), &["--title", "Special one"]);
        let plain = TestApp::spawn_at("AWWM.Test.RuleOrder", spot(1), &["--title", "Plain one"]);

        assert_eq!(
            engine
                .wait_pinned_by_engine(special.hwnd(0))?
                .rule_id
                .as_deref(),
            Some("r-specific")
        );
        assert_eq!(
            engine
                .wait_pinned_by_engine(plain.hwnd(0))?
                .rule_id
                .as_deref(),
            Some("r-generic")
        );
        assert_eq!(alpha(special.hwnd(0)), Some(opacity_to_alpha(40)));
        assert_eq!(alpha(plain.hwnd(0)), Some(opacity_to_alpha(90)));
        Ok(())
    });
}

#[test]
fn rules_ignore_tool_windows() {
    let _guard = setup();
    let engine = TestEngine::start(with_rules(vec![testapp_rule("r-tool", true, 100)]));

    let app = TestApp::spawn_at("AWWM.Test.RuleTool", spot(0), &["--tool"]);

    std::thread::sleep(QUIET_PERIOD);
    assert!(
        engine.managed(app.hwnd(0)).is_none(),
        "tool windows are not application windows"
    );
    assert!(!is_topmost(app.hwnd(0)));
}

#[test]
fn management_disabled_suspends_rules() {
    let _guard = setup();
    let mut s = with_rules(vec![testapp_rule("r-master", true, 100)]);
    s.general.management_enabled = false;
    let engine = TestEngine::start(s.clone());
    let app = TestApp::spawn_pinnable("AWWM.Test.RuleMaster", spot(0), &[]);
    let h = app.hwnd(0);
    std::thread::sleep(QUIET_PERIOD);
    assert!(engine.managed(h).is_none());

    s.general.management_enabled = true;
    engine.apply_settings(s);

    assert!(
        engine.managed(h).is_some(),
        "turning management on applies the rules"
    );
    assert!(is_topmost(h));
}

#[test]
fn rule_whose_pin_is_refused_keeps_it_pending_with_one_notice() {
    let _guard = setup();
    let engine = TestEngine::start(with_rules(vec![testapp_rule("r-veto", true, 80)]));

    // The app vetoes every move into the topmost band (WM_WINDOWPOSCHANGING) for 3 s,
    // the same way Windows refuses topmost while a full-screen app has the foreground.
    let app = TestApp::spawn_at(
        "AWWM.Test.RuleVeto",
        spot(0),
        &["--refuse-topmost-ms", "3000"],
    );
    let h = app.hwnd(0);

    let info = engine.wait_managed(h);
    assert!(
        info.pinned && info.pin_pending,
        "pin is kept pending: {info:?}"
    );
    assert!(!is_topmost(h));
    assert!(
        is_layered(h),
        "the rule's opacity is applied even while the pin is pending"
    );
    std::thread::sleep(QUIET_PERIOD);
    assert_eq!(
        engine.notices_for(h).len(),
        1,
        "exactly one notice: {:?}",
        engine.notices_for(h)
    );

    // Once the veto is over, an explicit pin applies it immediately.
    std::thread::sleep(Duration::from_millis(3200));
    let info = engine.handle().pin(h).expect("pin after the veto ended");
    assert!(info.pinned && !info.pin_pending);
    assert!(is_topmost(h));

    // Unpinning releases everything.
    engine.handle().unpin(h).expect("unpin");
    assert!(!is_topmost(h));
    assert!(!is_layered(h));
}

#[test]
fn unpinning_a_pending_rule_pin_leaves_no_trace() {
    let _guard = setup();
    let engine = TestEngine::start(with_rules(vec![testapp_rule("r-veto2", true, 70)]));
    let app = TestApp::spawn_at(
        "AWWM.Test.RuleVeto2",
        spot(0),
        &["--refuse-topmost-ms", "600000"],
    );
    let h = app.hwnd(0);
    let info = engine.wait_managed(h);
    assert!(info.pin_pending);
    engine.handle().unpin(h).expect("unpin");
    assert!(engine.managed(h).is_none());
    assert!(!is_topmost(h));
    assert!(!is_layered(h));
}
