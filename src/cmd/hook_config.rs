use crate::cli::{Cli, Commands, HookConfigArgs, HookConfigFormat, HookConfigFrom};
use crate::utils;
use anyhow::anyhow;
use clap::Parser;
use clap::error::ErrorKind;

pub(crate) fn run(args: &HookConfigArgs) -> anyhow::Result<crate::config::ShellHooksConfig> {
    let shell_hooks = collect_hook_config(args)?;
    match args.format {
        HookConfigFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&shell_hooks)?);
        }
    }
    Ok(shell_hooks)
}

fn collect_hook_config(args: &HookConfigArgs) -> anyhow::Result<crate::config::ShellHooksConfig> {
    let mut override_value = utils::shell_hooks_override(
        utils::resolve_bool_override(args.emit_hooks, args.no_emit_hooks),
        utils::resolve_bool_override(args.source_hooks, args.no_source_hooks),
    );

    if let Some(from) = &args.from {
        let from_override = hook_override_from_from_arg(from, &args.passthrough)?;
        if let Some(value) = from_override.emit {
            override_value.emit = Some(value);
        }
        if let Some(value) = from_override.source {
            override_value.source = Some(value);
        }
    }

    utils::resolve_shell_hooks_with_override(override_value)
}

fn hook_override_from_from_arg(
    from: &HookConfigFrom,
    passthrough: &[String],
) -> anyhow::Result<utils::ShellHooksOverride> {
    let argv = build_from_argv(from, passthrough);
    let parsed = match Cli::try_parse_from(argv) {
        Ok(parsed) => parsed,
        Err(err) => {
            if is_display_help_or_version(&err) {
                return Ok(utils::ShellHooksOverride::default());
            }
            return Err(anyhow!(err.to_string()));
        }
    };

    match parsed.command {
        Commands::Install(args) => Ok(utils::shell_hooks_override(
            utils::resolve_bool_override(args.emit_hooks, args.no_emit_hooks),
            None,
        )),
        Commands::Upgrade(args) => Ok(utils::shell_hooks_override(
            utils::resolve_bool_override(args.emit_hooks, args.no_emit_hooks),
            None,
        )),
        Commands::Uninstall(args) => Ok(utils::shell_hooks_override(
            utils::resolve_bool_override(args.emit_hooks, args.no_emit_hooks),
            None,
        )),
        other => anyhow::bail!("Unsupported --from target: {:?}", other),
    }
}

fn build_from_argv(from: &HookConfigFrom, passthrough: &[String]) -> Vec<String> {
    let subcmd = match from {
        HookConfigFrom::Install => "install",
        HookConfigFrom::Update => "upgrade",
        HookConfigFrom::Upgrade => "upgrade",
        HookConfigFrom::Uninstall => "uninstall",
        HookConfigFrom::Remove => "uninstall",
    };

    let mut argv = Vec::with_capacity(passthrough.len() + 2);
    argv.push("pez".to_string());
    argv.push(subcmd.to_string());
    argv.extend_from_slice(passthrough);
    argv
}

fn is_display_help_or_version(err: &clap::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::tests_support::env::TestEnvironmentSetup;
    use crate::tests_support::log::env_lock;

    fn with_env<F: FnOnce() -> anyhow::Result<()>>(env: &TestEnvironmentSetup, f: F) {
        let _lock = env_lock().lock().unwrap();
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
        }
        let result = f();
        unsafe {
            if let Some(v) = prev_pc {
                std::env::set_var("PEZ_CONFIG_DIR", v);
            } else {
                std::env::remove_var("PEZ_CONFIG_DIR");
            }
        }
        result.unwrap();
    }

    #[test]
    fn collect_hook_config_uses_config_defaults() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::Config {
            shell_hooks: config::ShellHooksConfig {
                emit: true,
                source: false,
            },
            plugins: None,
        });

        with_env(&env, || {
            let hooks = collect_hook_config(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: false,
                no_emit_hooks: false,
                source_hooks: false,
                no_source_hooks: false,
                from: None,
                passthrough: vec![],
            })?;
            assert!(hooks.emit);
            assert!(!hooks.source);
            Ok(())
        });
    }

    #[test]
    fn collect_hook_config_applies_direct_source_override() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());

        with_env(&env, || {
            let hooks = collect_hook_config(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: false,
                no_emit_hooks: false,
                source_hooks: true,
                no_source_hooks: false,
                from: None,
                passthrough: vec![],
            })?;
            assert!(!hooks.emit);
            assert!(hooks.source);
            Ok(())
        });
    }

    #[test]
    fn collect_hook_config_from_install_passthrough_overrides_emit() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());

        with_env(&env, || {
            let hooks = collect_hook_config(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: false,
                no_emit_hooks: false,
                source_hooks: false,
                no_source_hooks: false,
                from: Some(HookConfigFrom::Install),
                passthrough: vec!["--emit-hooks".into(), "owner/repo".into()],
            })?;
            assert!(hooks.emit);
            assert!(!hooks.source);
            Ok(())
        });
    }

    #[test]
    fn from_passthrough_overrides_direct_emit_override() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::Config {
            shell_hooks: config::ShellHooksConfig {
                emit: true,
                source: false,
            },
            plugins: None,
        });

        with_env(&env, || {
            let hooks = collect_hook_config(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: true,
                no_emit_hooks: false,
                source_hooks: false,
                no_source_hooks: false,
                from: Some(HookConfigFrom::Install),
                passthrough: vec!["--no-emit-hooks".into(), "owner/repo".into()],
            })?;
            assert!(!hooks.emit);
            Ok(())
        });
    }

    #[test]
    fn run_returns_non_default_config() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::Config {
            shell_hooks: config::ShellHooksConfig {
                emit: true,
                source: false,
            },
            plugins: None,
        });

        with_env(&env, || {
            let hooks = run(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: false,
                no_emit_hooks: false,
                source_hooks: false,
                no_source_hooks: false,
                from: None,
                passthrough: vec![],
            })?;
            assert!(hooks.emit);
            assert!(!hooks.source);
            Ok(())
        });
    }

    #[test]
    fn collect_hook_config_errors_on_invalid_passthrough_args() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());

        with_env(&env, || {
            let err = collect_hook_config(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: false,
                no_emit_hooks: false,
                source_hooks: false,
                no_source_hooks: false,
                from: Some(HookConfigFrom::Install),
                passthrough: vec!["--nope".into()],
            })
            .expect_err("invalid passthrough should fail");
            assert!(err.to_string().contains("unexpected argument '--nope'"));
            Ok(())
        });
    }

    #[test]
    fn collect_hook_config_ignores_help_passthrough_args() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::Config {
            shell_hooks: config::ShellHooksConfig {
                emit: true,
                source: false,
            },
            plugins: None,
        });

        with_env(&env, || {
            let hooks = collect_hook_config(&HookConfigArgs {
                format: HookConfigFormat::Json,
                emit_hooks: false,
                no_emit_hooks: false,
                source_hooks: false,
                no_source_hooks: false,
                from: Some(HookConfigFrom::Install),
                passthrough: vec!["--help".into()],
            })?;
            assert!(hooks.emit);
            assert!(!hooks.source);
            Ok(())
        });
    }
}
