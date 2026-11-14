use crate::i18n::Lang;
use std::io::{self, Write};

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const GREEN: &str = "\x1b[32m";
pub const CYAN: &str = "\x1b[36m";
pub const YELLOW: &str = "\x1b[33m";
pub const RED: &str = "\x1b[31m";
pub const MAGENTA: &str = "\x1b[35m";

pub fn info(tag: &str, msg: &str) {
    clear_progress_line();
    let label = tag.to_lowercase();
    println!("  {DIM}{label:<7}{RESET} {msg}");
}

pub fn success(_tag: &str, msg: &str) {
    clear_progress_line();
    println!("{GREEN}✓{RESET} {BOLD}{msg}{RESET}");
}

pub fn warn(_tag: &str, msg: &str) {
    clear_progress_line();
    eprintln!("{YELLOW}!{RESET} {msg}");
}

pub fn error(msg: &str) {
    clear_progress_line();
    eprintln!("{RED}✗{RESET} {msg}");
}

pub fn hit_line(_idx: usize, repo_display: &str, suite: &str, extra: &str) {
    clear_progress_line();
    if extra.is_empty() {
        println!("  {DIM}•{RESET} {BOLD}{repo_display}{RESET} {DIM}{suite}{RESET}");
    } else {
        println!("  {DIM}•{RESET} {BOLD}{repo_display}{RESET} {DIM}{suite} · {extra}{RESET}");
    }
}

pub fn get_line(_idx: usize, repo_display: &str, suite: &str, extra: &str) {
    clear_progress_line();
    if extra.is_empty() {
        println!("  {DIM}↓{RESET} {BOLD}{repo_display}{RESET} {DIM}{suite}{RESET}");
    } else {
        println!("  {DIM}↓{RESET} {BOLD}{repo_display}{RESET} {DIM}{suite} · {extra}{RESET}");
    }
}

pub fn err_line(_idx: usize, repo_display: &str, suite: &str, reason: &str) {
    clear_progress_line();
    println!("  {RED}✗{RESET} {BOLD}{repo_display}{RESET} {DIM}{suite}{RESET} {RED}{reason}{RESET}");
}

/// Prints a clean, minimalist AsthelOS package action line
pub fn action_line(
    action: &str,
    _color: &str,
    _idx: usize,
    origin: &str,
    pkg_name: &str,
    version: &str,
    extra: &str,
) {
    clear_progress_line();
    let verb = action.to_lowercase();
    if extra.is_empty() {
        println!("  {DIM}{verb:<7}{RESET} {BOLD}{pkg_name}{RESET} {DIM}{version} · {origin}{RESET}");
    } else {
        println!(
            "  {DIM}{verb:<7}{RESET} {BOLD}{pkg_name}{RESET} {DIM}{version} · {origin} ({extra}){RESET}"
        );
    }
}

/// Minimalist hairline progress bar for AsthelOS `pkg`
pub fn draw_progress_bar(_lang: Lang, current: usize, total: usize) {
    let pct = if total == 0 {
        100
    } else {
        ((current * 100) / total).min(100)
    };
    let width = 28usize;
    let filled = (pct * width) / 100;
    let empty = width.saturating_sub(filled);
    let bar_fill = "━".repeat(filled);
    let bar_empty = "─".repeat(empty);

    print!(
        "\r\x1b[2K  {DIM}pkg{RESET}  {BOLD}{bar_fill}{RESET}{DIM}{bar_empty}{RESET}  {DIM}{pct:>3}%{RESET}"
    );
    let _ = io::stdout().flush();
}

pub fn finish_progress_bar(lang: Lang) {
    draw_progress_bar(lang, 100, 100);
    std::thread::sleep(std::time::Duration::from_millis(60));
    clear_progress_line();
}

pub fn clear_progress_line() {
    print!("\r\x1b[2K");
    let _ = io::stdout().flush();
}
