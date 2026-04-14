use crate::api::Library;
use owo_colors::OwoColorize;

pub fn format_num(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn row(lib: &Library) -> String {
    let name = lib.name();
    let stars = format_num(lib.stars());
    let dls = format_num(lib.weekly_downloads());
    let archived = if lib.is_archived() {
        format!(" {}", "[archived]".red())
    } else if lib.unmaintained {
        format!(" {}", "[unmaintained]".yellow())
    } else {
        String::new()
    };
    let platforms = {
        let mut p = Vec::new();
        if lib.ios {
            p.push("ios");
        }
        if lib.android {
            p.push("and");
        }
        if lib.web {
            p.push("web");
        }
        p.join(",")
    };
    let desc = lib.description();
    let desc_trunc = if desc.len() > 70 {
        format!("{}…", &desc[..70])
    } else {
        desc.to_string()
    };
    format!(
        "{:<40} {:>7} {:>8}/wk  [{:>11}]{}\n    {}",
        name.bold(),
        format!("★{stars}").yellow(),
        dls.cyan(),
        platforms.dimmed(),
        archived,
        desc_trunc.dimmed()
    )
}

pub fn detailed(lib: &Library) {
    println!("{}", lib.name().bold());
    if !lib.description().is_empty() {
        println!("  {}", lib.description());
    }
    println!();
    println!("  {} ★{}  ⭳ {}/week  score {}",
        "stats:".dimmed(),
        format_num(lib.stars()),
        format_num(lib.weekly_downloads()),
        lib.score.unwrap_or(0.0) as i64,
    );
    let mut flags = Vec::new();
    if lib.ios { flags.push("iOS"); }
    if lib.android { flags.push("Android"); }
    if lib.web { flags.push("Web"); }
    if lib.expo { flags.push("Expo"); }
    println!("  {} {}", "runs on:".dimmed(), flags.join(", "));

    if let Some(gh) = &lib.github {
        if gh.has_types {
            println!("  {} yes", "typescript:".dimmed());
        }
        if matches!(lib.supports_new_architecture(), Some(true)) {
            println!("  {} yes", "new arch:".dimmed());
        }
        if let Some(lic) = &gh.license {
            if let Some(id) = &lic.spdx_id {
                println!("  {} {}", "license:".dimmed(), id);
            }
        }
    }

    if let Some(npm) = &lib.npm {
        if let Some(v) = &npm.latest_release {
            println!("  {} {}", "version:".dimmed(), v);
        }
    }

    if let Some(pushed) = lib.pushed_at() {
        println!("  {} {}", "last push:".dimmed(), pushed);
    }

    if lib.is_archived() {
        println!("  {} {}", "status:".dimmed(), "ARCHIVED".red().bold());
    } else if lib.unmaintained {
        println!("  {} {}", "status:".dimmed(), "UNMAINTAINED".yellow().bold());
    }

    if let Some(url) = &lib.github_url {
        println!("  {} {}", "github:".dimmed(), url);
    }
}
