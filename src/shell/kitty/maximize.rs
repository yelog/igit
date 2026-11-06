use super::{run_output, Target};
use serde::Deserialize;
use std::{
    process::Command,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct OsWindow {
    tabs: Vec<Tab>,
}

#[derive(Deserialize)]
struct Tab {
    id: u64,
    layout: String,
    windows: Vec<Window>,
    // Overlays share the underlying window's layout group.
    groups: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct Window {
    id: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct SourceTab {
    id: u64,
    layout: String,
    groups: usize,
}

fn source_tab(output: &[u8], window_id: u64) -> Result<SourceTab, String> {
    let windows: Vec<OsWindow> = serde_json::from_slice(output)
        .map_err(|error| format!("Kitty returned invalid pane state: {error}"))?;
    let tab = windows
        .into_iter()
        .flat_map(|window| window.tabs)
        .find(|tab| tab.windows.iter().any(|window| window.id == window_id))
        .ok_or("Kitty source window was not found")?;
    Ok(SourceTab {
        id: tab.id,
        layout: tab.layout,
        groups: tab.groups.as_ref().map_or(tab.windows.len(), Vec::len),
    })
}

pub(super) fn toggle(
    target: &Target,
    cancelled: &AtomicBool,
    timeout: Duration,
) -> Result<Option<bool>, String> {
    let start = Instant::now();
    toggle_with(target, |command| {
        run_output(command, cancelled, timeout.saturating_sub(start.elapsed()))
    })
}

fn toggle_with(
    target: &Target,
    mut execute: impl FnMut(Command) -> Result<Vec<u8>, String>,
) -> Result<Option<bool>, String> {
    let tab = source_tab(&execute(target.remote_command(&["ls"]))?, target.window)?;
    if tab.groups < 2 {
        return Ok(None);
    }
    let maximized = tab.layout != "stack";
    let tab_match = format!("id:{}", tab.id);
    let command = if maximized {
        target.remote_command(&["goto-layout", "--match", &tab_match, "stack"])
    } else {
        target.remote_command(&["last-used-layout", "--match", &tab_match])
    };
    execute(command)?;
    let updated = source_tab(&execute(target.remote_command(&["ls"]))?, target.window)?;
    if updated.id != tab.id {
        return Err("Kitty source window changed tabs during maximize".into());
    }
    // The source may have become the only group while the command was running.
    if updated.groups < 2 {
        return Ok(None);
    }
    if (updated.layout == "stack") != maximized {
        return Err("Kitty did not reach the requested layout".into());
    }
    Ok(Some(maximized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn target() -> Target {
        Target {
            socket: "unix:/test socket".into(),
            window: 4,
        }
    }

    fn state(tab_id: u64, layout: &str, ids: &[u64]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!([
            {"tabs": [{"id": 10, "layout": "stack", "is_active": true,
                "windows": [{"id": 1}, {"id": 2}]}]},
            {"tabs": [{"id": tab_id, "layout": layout, "is_active": false,
                "windows": ids.iter().map(|id| serde_json::json!({"id": id})).collect::<Vec<_>>() } ]}
        ]))
        .unwrap()
    }

    fn replay(
        outputs: impl IntoIterator<Item = Result<Vec<u8>, String>>,
    ) -> (Result<Option<bool>, String>, Vec<Vec<String>>) {
        let mut outputs: VecDeque<_> = outputs.into_iter().collect();
        let mut commands = vec![];
        let result = toggle_with(&target(), |command| {
            assert_eq!(command.get_program(), "kitten");
            commands.push(
                command
                    .get_args()
                    .map(|arg| arg.to_str().unwrap().to_owned())
                    .collect(),
            );
            outputs.pop_front().expect("unexpected Kitty command")
        });
        assert!(outputs.is_empty(), "an expected Kitty command was skipped");
        (result, commands)
    }

    #[test]
    fn smart_maximize_finds_source_tab_across_os_windows() {
        let output = state(20, "splits", &[3, 4]);
        assert_eq!(
            source_tab(&output, 4).unwrap(),
            SourceTab {
                id: 20,
                layout: "splits".into(),
                groups: 2,
            }
        );
        assert_eq!(source_tab(&output, 2).unwrap().id, 10);
        assert!(source_tab(&output, 99).unwrap_err().contains("not found"));
        assert!(source_tab(b"invalid JSON", 4)
            .unwrap_err()
            .contains("invalid pane state"));
        assert!(source_tab(br#"[{"tabs":[{"id":20}]}]"#, 4).is_err());
    }

    #[test]
    fn smart_maximize_counts_overlay_groups_and_legacy_windows() {
        let overlay = serde_json::to_vec(&serde_json::json!([{"tabs": [{
            "id": 20, "layout": "splits", "windows": [{"id": 3}, {"id": 4}],
            "groups": [{"id": 3, "windows": [3, 4]}]
        }]}]))
        .unwrap();
        assert_eq!(source_tab(&overlay, 4).unwrap().groups, 1);
        assert_eq!(source_tab(&state(20, "splits", &[4]), 4).unwrap().groups, 1);
        assert_eq!(
            source_tab(&state(20, "splits", &[3, 4]), 4).unwrap().groups,
            2
        );
        for output in [overlay, state(20, "stack", &[4])] {
            let (result, commands) = replay([Ok(output)]);
            assert_eq!(result, Ok(None));
            assert_eq!(commands, [["@", "--to", "unix:/test socket", "ls"]]);
        }
    }

    #[test]
    fn smart_maximize_changes_only_source_tab_and_verifies_layout() {
        for (initial, final_layout, action, expected) in [
            ("splits", "stack", "goto-layout", true),
            ("stack", "tall", "last-used-layout", false),
        ] {
            let (result, commands) = replay([
                Ok(state(20, initial, &[3, 4])),
                Ok(vec![]),
                Ok(state(20, final_layout, &[3, 4])),
            ]);
            assert_eq!(result, Ok(Some(expected)));
            assert_eq!(commands[0], ["@", "--to", "unix:/test socket", "ls"]);
            let mut expected_action =
                vec!["@", "--to", "unix:/test socket", action, "--match", "id:20"];
            if expected {
                expected_action.push("stack");
            }
            assert_eq!(commands[1], expected_action);
            assert_eq!(commands[2], commands[0]);
        }
    }

    #[test]
    fn smart_maximize_rejects_unverified_or_moved_targets() {
        for (initial, updated) in [
            ("splits", state(20, "splits", &[3, 4])),
            ("stack", state(20, "stack", &[3, 4])),
            ("splits", state(30, "stack", &[3, 4])),
            ("splits", state(30, "stack", &[4])),
            ("splits", state(20, "stack", &[3])),
            ("splits", b"invalid JSON".to_vec()),
        ] {
            let (result, commands) =
                replay([Ok(state(20, initial, &[3, 4])), Ok(vec![]), Ok(updated)]);
            assert!(result.is_err());
            assert_eq!(commands.len(), 3);
        }
    }

    #[test]
    fn smart_maximize_uses_local_toggle_if_own_tab_becomes_single_group() {
        let (result, _) = replay([
            Ok(state(20, "splits", &[3, 4])),
            Ok(vec![]),
            Ok(state(20, "stack", &[4])),
        ]);
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn smart_maximize_stops_at_each_failed_remote_command() {
        for successful_commands in 0..3 {
            let mut outputs = vec![Ok(state(20, "splits", &[3, 4])), Ok(vec![])];
            outputs.truncate(successful_commands);
            outputs.push(Err("remote control refused".into()));
            let (result, commands) = replay(outputs);
            assert_eq!(result, Err("remote control refused".into()));
            assert_eq!(commands.len(), successful_commands + 1);
        }
    }
}
