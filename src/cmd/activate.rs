/// Fish activation script emitter.
///
/// Prints Fish wrapper code to stdout. Keep stdout clean of logs so it can be
/// piped into `source`.
pub(crate) fn run_fish() -> String {
    let script = fish_script();
    print!("{script}");
    script
}

fn fish_script() -> String {
    let version = env!("CARGO_PKG_VERSION");
    // Guard against multiple sourcing and wrap pez to emit events in-process.
    format!(
        r#"
set -l __pez_version "{version}"
if not set -q __pez_activate_version; or test "$__pez_activate_version" != "$__pez_version"
    set -g __pez_activate_version $__pez_version

    function __pez_fish_split_subcmd --description "Find subcommand and args"
        set -l args $argv
        set -l i 1
        set -l argc (count $args)
        while test $i -le $argc
            set -l arg $args[$i]
            if test "$arg" = "--"
                return 1
            end
            if string match -rq -- '^-[v]+$' "$arg"
                set i (math $i + 1)
                continue
            end
            switch $arg
            case '--verbose' '-V' '--version' '-h' '--help'
                set i (math $i + 1)
                continue
            case '--jobs'
                set i (math $i + 2)
                continue
            case '--jobs=*'
                set i (math $i + 1)
                continue
            end
            if string match -q -- '-*' "$arg"
                return 1
            end
            set -l subcmd $arg
            set -l subargs $args[(math $i + 1)..-1]
            echo $subcmd $subargs
            return 0
        end
        return 1
    end

    function __pez_fish_source_and_emit --description "Source conf.d and emit events" --argument-names phase from
        set -l passthrough $argv[3..-1]
        set -l paths (command pez files --dir conf.d --from $from -- $passthrough | sort)
        for path in $paths
            if test -f "$path"
                source "$path"
                set -l name (basename "$path" .fish)
                emit "$name"_"$phase"
            end
        end
    end

    function pez --wraps pez --description "pez with fish event hooks"
        set -l parsed (__pez_fish_split_subcmd $argv)
        if test (count $parsed) -eq 0
            command pez $argv
            return $status
        end
        set -l subcmd $parsed[1]
        set -l subargs $parsed[2..-1]
        switch $subcmd
        case install
            env PEZ_SUPPRESS_EMIT=1 command pez $argv
            set -l exit_status $status
            if test $exit_status -eq 0
                __pez_fish_source_and_emit install install $subargs
            end
            return $exit_status
        case update upgrade
            env PEZ_SUPPRESS_EMIT=1 command pez $argv
            set -l exit_status $status
            if test $exit_status -eq 0
                __pez_fish_source_and_emit update $subcmd $subargs
            end
            return $exit_status
        case uninstall remove
            if contains -- --stdin $subargs
                set -l stdin_file (cat | psub -f -s .pez_uninstall)
                cat $stdin_file | __pez_fish_source_and_emit uninstall $subcmd $subargs
                cat $stdin_file | env PEZ_SUPPRESS_EMIT=1 command pez $argv
                set -l exit_status $status
                return $exit_status
            end
            __pez_fish_source_and_emit uninstall $subcmd $subargs
            env PEZ_SUPPRESS_EMIT=1 command pez $argv
            set -l exit_status $status
            return $exit_status
        case '*'
            command pez $argv
        end
    end
end
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_contains_guard_and_suppress_flag() {
        let text = fish_script();
        assert!(text.contains("__pez_activate_version"));
        assert!(text.contains("__pez_version"));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("PEZ_SUPPRESS_EMIT"));
        assert!(text.contains("command pez files --dir conf.d --from"));
        assert!(text.contains("__pez_fish_split_subcmd"));
    }

    #[test]
    fn uninstall_emits_before_command() {
        let text = fish_script();
        let parts: Vec<&str> = text.split("case uninstall remove").collect();
        assert!(parts.len() > 1, "uninstall case missing");
        let segment = parts[1];
        let emit_pos = segment
            .find("__pez_fish_source_and_emit uninstall")
            .expect("emit call missing");
        let command_pos = segment
            .find("env PEZ_SUPPRESS_EMIT=1 command pez $argv")
            .expect("command call missing");
        assert!(
            emit_pos < command_pos,
            "emit must occur before uninstall command"
        );
    }

    #[test]
    fn targets_not_dropped() {
        // ensure we don't slice away the first target (no custom filter function)
        let text = fish_script();
        assert!(!text.contains("__pez_fish_filter_targets"));
        assert!(text.contains("set -l subargs $parsed[2..-1]"));
        assert!(text.contains("__pez_fish_source_and_emit install install $subargs"));
        assert!(text.contains("set -l passthrough $argv[3..-1]"));
    }

    #[test]
    fn global_flags_are_skipped() {
        let text = fish_script();
        assert!(text.contains("--jobs"));
        assert!(text.contains("--jobs=*"));
        assert!(text.contains("--verbose"));
        assert!(text.contains("^-[v]+$"));
    }

    #[test]
    fn uninstall_stdin_is_tapped() {
        let text = fish_script();
        assert!(text.contains("contains -- --stdin $subargs"));
        assert!(text.contains("psub -f -s .pez_uninstall"));
        assert!(text.contains("cat $stdin_file | __pez_fish_source_and_emit uninstall"));
        assert!(text.contains("cat $stdin_file | env PEZ_SUPPRESS_EMIT=1 command pez $argv"));
    }
}
