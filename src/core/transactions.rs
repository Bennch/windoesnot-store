use std::path::PathBuf;

use crate::core::models::{
    ActionKind, AurHelperKind, PackageSource, Settings, TerminalMode, TransactionAction,
    TransactionQueue,
};
use crate::core::runner::CommandSpec;

#[derive(Debug, Clone)]
pub struct TransactionPlan {
    pub commands: Vec<CommandSpec>,
}

pub fn plan_transactions(queue: &TransactionQueue, settings: &Settings) -> TransactionPlan {
    let mut commands = Vec::new();
    for action in &queue.actions {
        if let Some(cmd) = command_for_action(action, settings) {
            commands.push(cmd);
        }
    }
    TransactionPlan { commands }
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
                let mut args = vec![helper.clone(), "pacman".to_string(), "-S".to_string()];
                args.extend(noconfirm.clone());
                args.push(action.name.clone());
                Some(CommandSpec::new("pkexec", args))
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
            ActionKind::Install => Some(aur_command(settings.aur_helper, "-S", &action.name, &noconfirm, &helper)),
            ActionKind::Remove => Some(aur_command(settings.aur_helper, "-Rns", &action.name, &noconfirm, &helper)),
            ActionKind::Upgrade => Some(aur_command(settings.aur_helper, "-Syu", &action.name, &noconfirm, &helper)),
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

fn aur_command(helper: AurHelperKind, op: &str, pkg: &str, noconfirm: &[String], helper_path: &str) -> CommandSpec {
    let mut args = vec![op.to_string()];
    args.extend(noconfirm.to_vec());
    if op != "-Syu" {
        args.push(pkg.to_string());
    }

    // Best-effort: ask yay/paru to use pkexec + windoesnot-store-helper for pacman calls.
    // This respects the "privileged ops via helper" requirement when supported by the helper.
    args.push("--sudo".to_string());
    args.push("pkexec".to_string());
    args.push("--sudoflags".to_string());
    args.push(helper_path.to_string());

    CommandSpec::new(helper.as_str(), args)
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
