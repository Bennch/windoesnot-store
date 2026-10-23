use std::path::PathBuf;
use anyhow::{anyhow, Result};

use crate::core::models::{
    ActionKind, AurHelperKind, PackageSource, Settings, TerminalMode, TransactionAction,
    TransactionQueue,
};
use crate::core::runner::CommandSpec;

#[derive(Debug, Clone)]
pub struct TransactionPlan {
    pub commands: Vec<CommandSpec>,
}

pub fn all_upgrade_actions(aur_available: bool, flatpak_available: bool) -> Vec<TransactionAction> {
    let mut sources = vec![(PackageSource::Repo, "system")];
    if aur_available { sources.push((PackageSource::Aur, "aur")); }
    if flatpak_available { sources.push((PackageSource::Flatpak, "flatpak")); }
    sources.into_iter().map(|(source, name)| TransactionAction {
        name: name.into(), source, kind: ActionKind::Upgrade, origin: None,
    }).collect()
}

pub fn plan_transactions(queue: &TransactionQueue, settings: &Settings) -> Result<TransactionPlan> {
    let mut current = settings.clone();
    // Re-read immediately before execution: a desktop toggle may have changed
    // since the review dialog opened. Removal of existing apps remains allowed.
    current.aur_enabled = crate::core::cache::load_settings().aur_enabled;
    plan_with_helper(queue, &current, current.aur_helper.installed())
}

fn plan_with_helper(queue: &TransactionQueue, settings: &Settings, helper: Option<AurHelperKind>) -> Result<TransactionPlan> {
    if !settings.aur_enabled && queue.actions.iter().any(|action|
        action.source == PackageSource::Aur && action.kind != ActionKind::Remove)
    {
        return Err(anyhow!(crate::i18n::t("AUR is disabled. Enable it in Settings before installing or updating AUR packages.")));
    }
    let mut settings = settings.clone();
    if queue.actions.iter().any(|action| action.source == PackageSource::Aur) {
        settings.aur_helper = helper.ok_or_else(|| anyhow!(crate::i18n::t(
            "No AUR helper installed. Install paru or yay to use AUR."
        )))?;
    }
    if settings.aur_helper == AurHelperKind::Paru
        && queue.actions.iter().any(|action| action.source == PackageSource::Aur && action.kind != ActionKind::Remove)
    {
        return Ok(paru_plan(queue, &settings));
    }
    let mut commands = Vec::new();
    // Whether this plan already runs a full native upgrade before the current
    // action; a later AUR build then needs no separate preflight.
    let mut native_upgraded = false;
    let mut i = 0;
    while i < queue.actions.len() {
        let action = &queue.actions[i];
        if action.source == PackageSource::Repo && action.kind != ActionKind::Remove {
            // Preserve ordering around removals/AUR/Flatpak; combine each native
            // install/update block into one solver transaction, never a partial update.
            let mut targets = Vec::new();
            while let Some(next) = queue.actions.get(i) {
                if next.source != PackageSource::Repo || next.kind == ActionKind::Remove { break; }
                if next.kind == ActionKind::Install && !targets.contains(&next.name) {
                    targets.push(next.name.clone());
                }
                i += 1;
            }
            commands.push(repo_install_command(&settings, targets));
            native_upgraded = true;
            continue;
        }
        if action.source == PackageSource::Aur && action.kind != ActionKind::Remove && !native_upgraded {
            // Stop on an incompatible native base before starting any AUR build.
            // The existing runner aborts the queue on the first non-zero exit.
            commands.push(repo_install_command(&settings, Vec::new()));
            native_upgraded = true;
        }
        if let Some(cmd) = command_for_action(action, &settings) {
            commands.push(cmd);
        }
        i += 1;
    }
    Ok(TransactionPlan { commands })
}

/// With AUR builds and paru, one paru run does the native full upgrade
/// (including the queued repo packages) and then the AUR builds. polkit keeps
/// an authorization only for the process that asked for it, so a separate
/// store-helper upgrade before paru meant a second password prompt. paru
/// upgrades first and stops before building when pacman refuses, e.g. because
/// a WinDoesNot package pins Hyprland (tested in a VM).
fn paru_plan(queue: &TransactionQueue, settings: &Settings) -> TransactionPlan {
    let mut commands = Vec::new();
    let mut targets = Vec::new();
    let mut slot = None;
    for action in &queue.actions {
        let joins = action.kind != ActionKind::Remove
            && matches!(action.source, PackageSource::Repo | PackageSource::Aur);
        if !joins {
            commands.extend(command_for_action(action, settings));
            continue;
        }
        // Updates are covered by the full upgrade itself.
        if action.kind == ActionKind::Install {
            let target = match action.source {
                PackageSource::Aur => format!("aur/{}", action.name),
                _ => action.name.clone(),
            };
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        // The combined run takes the place of the first native or AUR action.
        if slot.is_none() {
            slot = Some(commands.len());
        }
    }
    let mut args = vec!["-Syu".to_string(), "--needed".to_string()];
    if settings.allow_noconfirm || settings.terminal_mode == TerminalMode::Integrated {
        args.push("--noconfirm".to_string());
    }
    args.extend(targets);
    let paru = with_privileged_helper(CommandSpec::new(AurHelperKind::Paru.as_str(), args), &helper_path());
    commands.insert(slot.unwrap_or(0), paru);
    TransactionPlan { commands }
}

fn repo_install_command(settings: &Settings, targets: Vec<String>) -> CommandSpec {
    let mut args = vec![helper_path(), "pacman".to_string(), "-Syu".to_string(), "--needed".to_string()];
    if settings.allow_noconfirm || settings.terminal_mode == TerminalMode::Integrated {
        args.push("--noconfirm".to_string());
    }
    args.extend(targets);
    CommandSpec::new("pkexec", args)
}

pub fn command_for_action(action: &TransactionAction, settings: &Settings) -> Option<CommandSpec> {
    // Integrated logs run through pipes, so interactive package prompts can block forever.
    // Force non-interactive mode there; user setting still controls external-terminal mode.
    let noninteractive = settings.allow_noconfirm || settings.terminal_mode == TerminalMode::Integrated;
    let mut noconfirm = Vec::new();
    if noninteractive {
        noconfirm.push("--noconfirm".to_string());
    }
    let helper = helper_path();

    match action.source {
        PackageSource::Repo => match action.kind {
            ActionKind::Install => {
                Some(repo_install_command(settings, vec![action.name.clone()]))
            }
            ActionKind::Remove => {
                let mut args = vec![helper.clone(), "pacman".to_string(), "-Rns".to_string()];
                args.extend(noconfirm.clone());
                args.push(action.name.clone());
                Some(CommandSpec::new("pkexec", args))
            }
            ActionKind::Upgrade => {
                let mut args = vec![helper.clone(), "pacman".to_string(), "-Syu".to_string()];
                args.extend(noconfirm.clone());
                Some(CommandSpec::new("pkexec", args))
            }
        },
        PackageSource::Aur => match action.kind {
            ActionKind::Install => Some(aur_command(settings.aur_helper, "-Syu", Some(&action.name), &noconfirm, &helper)),
            ActionKind::Remove => Some(aur_command(settings.aur_helper, "-Rns", Some(&action.name), &noconfirm, &helper)),
            ActionKind::Upgrade => Some(aur_command(settings.aur_helper, "-Syu", None, &noconfirm, &helper)),
        },
        PackageSource::Flatpak => match action.kind {
            ActionKind::Install => {
                let mut args = vec!["install".to_string(), "-y".to_string()];
                if let Some(origin) = &action.origin {
                    if !origin.is_empty() {
                        args.push(origin.clone());
                    }
                }
                args.push(action.name.clone());
                Some(CommandSpec::new("flatpak", args))
            }
            ActionKind::Remove => {
                let args = vec!["uninstall".to_string(), "-y".to_string(), action.name.clone()];
                Some(CommandSpec::new("flatpak", args))
            }
            ActionKind::Upgrade => {
                let mut args = vec!["update".to_string(), "-y".to_string()];
                if action.name != "flatpak" && action.name != "all" && !action.name.is_empty() {
                    args.push(action.name.clone());
                }
                Some(CommandSpec::new("flatpak", args))
            }
        },
    }
}

fn aur_command(helper: AurHelperKind, op: &str, pkg: Option<&str>, noconfirm: &[String], helper_path: &str) -> CommandSpec {
    let mut args = vec![op.to_string()];
    if op == "-Syu" { args.push("--needed".to_string()); }
    args.extend(noconfirm.to_vec());
    if let Some(pkg) = pkg {
        // Removal works on installed names; installs must not resolve to a repo
        // package that merely provides the same name (pfetch -> pfetch-rs).
        args.push(if op == "-Rns" { pkg.to_string() } else { format!("aur/{pkg}") });
    }

    with_privileged_helper(CommandSpec::new(helper.as_str(), args), helper_path)
}

/// Lets yay/paru run their pacman calls through pkexec and the store helper.
fn with_privileged_helper(mut command: CommandSpec, helper_path: &str) -> CommandSpec {
    for arg in ["--sudo", "pkexec", "--sudoflags", helper_path] {
        command.args.push(arg.to_string());
    }
    command
}

fn helper_path() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let mut candidate = PathBuf::from(dir);
            candidate.push("windoesnot-store-helper");
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }
    "/usr/bin/windoesnot-store-helper".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn enabled_settings() -> Settings { Settings { aur_enabled: true, ..Settings::default() } }

    #[test]
    fn disabled_aur_rejects_stale_install_queue_but_allows_removal() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Install, "htop"),
            action(PackageSource::Aur, ActionKind::Install, "pfetch"),
        ] };
        assert!(plan_with_helper(&queue, &Settings::default(), Some(AurHelperKind::Paru)).is_err());
        let removal = TransactionQueue { actions: vec![action(PackageSource::Aur, ActionKind::Remove, "pfetch")] };
        assert!(plan_with_helper(&removal, &Settings::default(), Some(AurHelperKind::Paru)).is_ok());
    }


    fn action(source: PackageSource, kind: ActionKind, name: &str) -> TransactionAction {
        TransactionAction { name: name.into(), source, kind, origin: None }
    }

    #[test]
    fn native_installs_and_update_share_one_transaction() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Install, "firefox"),
            action(PackageSource::Repo, ActionKind::Upgrade, "pacman"),
            action(PackageSource::Repo, ActionKind::Install, "vlc"),
            action(PackageSource::Repo, ActionKind::Install, "firefox"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), None).unwrap();
        assert_eq!(plan.commands.len(), 1);
        let cmd = &plan.commands[0];
        assert_eq!(cmd.program, "pkexec");
        assert_eq!(&cmd.args[1..], ["pacman", "-Syu", "--needed", "--noconfirm", "firefox", "vlc"]);
    }

    #[test]
    fn removals_are_ordering_boundaries() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Install, "firefox"),
            action(PackageSource::Repo, ActionKind::Remove, "vlc"),
            action(PackageSource::Repo, ActionKind::Install, "gimp"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), None).unwrap();
        assert_eq!(plan.commands.len(), 3);
        assert_eq!(plan.commands[0].args[2], "-Syu");
        assert_eq!(plan.commands[1].args[2], "-Rns");
        assert_eq!(plan.commands[2].args[2], "-Syu");
    }

    #[test]
    fn yay_aur_install_requires_native_upgrade_first_and_keeps_target() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Aur, ActionKind::Install, "localsend-bin"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), Some(AurHelperKind::Yay)).unwrap();
        assert_eq!(plan.commands.len(), 2);
        assert_eq!(plan.commands[0].program, "pkexec");
        assert_eq!(plan.commands[0].args[2], "-Syu");
        let aur = &plan.commands[1];
        assert_eq!(aur.program, "yay");
        // aur/ keeps the helper from picking a repo package that provides the name.
        assert_eq!(&aur.args[..4], ["-Syu", "--needed", "--noconfirm", "aur/localsend-bin"]);
    }

    #[test]
    fn yay_one_native_upgrade_covers_later_aur_builds() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Install, "htop"),
            action(PackageSource::Aur, ActionKind::Install, "pfetch"),
            action(PackageSource::Aur, ActionKind::Install, "localsend-bin"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), Some(AurHelperKind::Yay)).unwrap();
        let programs: Vec<_> = plan.commands.iter().map(|command| command.program.as_str()).collect();
        assert_eq!(programs, ["pkexec", "yay", "yay"]);
        assert!(plan.commands[0].args.iter().any(|arg| arg == "htop"));
    }

    #[test]
    fn paru_runs_native_and_aur_installs_in_one_process() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Install, "htop"),
            action(PackageSource::Aur, ActionKind::Install, "pfetch"),
            action(PackageSource::Repo, ActionKind::Upgrade, "system"),
            action(PackageSource::Aur, ActionKind::Install, "pfetch"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), Some(AurHelperKind::Paru)).unwrap();
        assert_eq!(plan.commands.len(), 1);
        let paru = &plan.commands[0];
        assert_eq!(paru.program, "paru");
        assert_eq!(&paru.args[..5], ["-Syu", "--needed", "--noconfirm", "htop", "aur/pfetch"]);
        assert_eq!(&paru.args[5..], ["--sudo", "pkexec", "--sudoflags", helper_path().as_str()]);
    }

    #[test]
    fn paru_plan_keeps_removals_and_flatpak_in_order() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Remove, "vlc"),
            action(PackageSource::Aur, ActionKind::Install, "pfetch"),
            action(PackageSource::Flatpak, ActionKind::Install, "org.gimp.GIMP"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), Some(AurHelperKind::Paru)).unwrap();
        let programs: Vec<_> = plan.commands.iter().map(|command| command.program.as_str()).collect();
        assert_eq!(programs, ["pkexec", "paru", "flatpak"]);
        assert_eq!(plan.commands[0].args[2], "-Rns");
    }

    #[test]
    fn aur_removal_uses_the_installed_name() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Aur, ActionKind::Remove, "localsend-bin"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), Some(AurHelperKind::Paru)).unwrap();
        let aur = plan.commands.iter().find(|command| command.program == "paru").unwrap();
        assert!(aur.args.iter().any(|arg| arg == "localsend-bin"));
        assert!(!aur.args.iter().any(|arg| arg.starts_with("aur/")));
    }

    #[test]
    fn aur_upgrade_has_no_synthetic_package_target() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Aur, ActionKind::Upgrade, "all"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), Some(AurHelperKind::Yay)).unwrap();
        assert_eq!(plan.commands[1].program, "yay");
        assert!(!plan.commands[1].args.iter().any(|arg| arg == "all"));
    }

    #[test]
    fn absent_aur_helper_rejects_whole_plan_before_any_execution() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Repo, ActionKind::Install, "firefox"),
            action(PackageSource::Aur, ActionKind::Install, "localsend-bin"),
        ] };
        assert!(plan_with_helper(&queue, &enabled_settings(), None).is_err());
    }

    #[test]
    fn flatpak_does_not_force_native_upgrade() {
        let queue = TransactionQueue { actions: vec![
            action(PackageSource::Flatpak, ActionKind::Install, "org.videolan.VLC"),
        ] };
        let plan = plan_with_helper(&queue, &enabled_settings(), None).unwrap();
        assert_eq!(plan.commands.len(), 1);
        assert_eq!(plan.commands[0].program, "flatpak");
    }

    #[test]
    fn update_all_works_without_optional_package_managers() {
        let actions = all_upgrade_actions(false, false);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].source, PackageSource::Repo);
        assert!(plan_with_helper(&TransactionQueue { actions }, &enabled_settings(), None).is_ok());
        assert_eq!(all_upgrade_actions(true, true).len(), 3);
    }

    #[test]
    fn helper_detection_preserves_available_preference_and_handles_missing_helpers() {
        assert_eq!(AurHelperKind::Yay.resolve_with(|name| name == "paru"), Some(AurHelperKind::Paru));
        assert_eq!(AurHelperKind::Paru.resolve_with(|name| name == "yay"), Some(AurHelperKind::Yay));
        assert_eq!(AurHelperKind::Paru.resolve_with(|_| true), Some(AurHelperKind::Paru));
        assert_eq!(AurHelperKind::Yay.resolve_with(|_| true), Some(AurHelperKind::Yay));
        assert_eq!(AurHelperKind::Yay.resolve_with(|_| false), None);
    }
}
