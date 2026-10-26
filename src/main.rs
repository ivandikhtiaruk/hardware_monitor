use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, 
        EnterAlternateScreen, LeaveAlternateScreen
    },
};
use std::{
    io::{stdout, Write},
    thread,
    time::{Duration, Instant},
};
use sysinfo::System;

/// Генерація візуального прогрес-бару з кольоровою індикацією
fn make_progress_bar(percentage: f32, width: usize) -> (String, Color) {
    let clamped = percentage.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

    let color = if clamped < 50.0 {
        Color::Green
    } else if clamped < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    (bar, color)
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.2} GiB", bytes as f64 / GIB)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Ініціалізація повноекранного термінала
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    // 2. Ініціалізація sysinfo з прямим запуском лічильників CPU під Windows
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    thread::sleep(Duration::from_millis(250));
    sys.refresh_cpu_usage();

    let refresh_rate = Duration::from_millis(1000);
    let mut last_tick = Instant::now();

    loop {
        // Перевірка клавіш виходу (q або Esc)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= refresh_rate {
            // Пряме оновлення CPU та пам'яті через рідні методи
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            last_tick = Instant::now();

            // Очищення екрана перед новим кадром
            execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;

            // Заголовок
            execute!(out, SetForegroundColor(Color::Cyan))?;
            writeln!(out, "================================================================================")?;
            writeln!(out, "                   HARDWARE RESOURCE MONITOR (Variant 14)                       ")?;
            writeln!(out, "                    Press 'q' or 'Esc' to exit the monitor                      ")?;
            writeln!(out, "================================================================================")?;
            execute!(out, ResetColor)?;

            // 1. Інтегральне навантаження CPU
            let global_cpu = sys.global_cpu_info().cpu_usage();
            let (cpu_bar, cpu_color) = make_progress_bar(global_cpu, 25);

            execute!(out, SetForegroundColor(Color::White))?;
            write!(out, " Total CPU Load:  ")?;
            execute!(out, SetForegroundColor(cpu_color))?;
            writeln!(out, "{} {:5.1}%", cpu_bar, global_cpu)?;
            execute!(out, ResetColor)?;

            // 2. Вивід 28 ядер у ДВІ колонки
            writeln!(out, " Per Core Breakdown (2 columns):")?;
            let cpus: Vec<_> = sys.cpus().iter().enumerate().collect();
            let half = (cpus.len() + 1) / 2;

            for i in 0..half {
                // Ліва колонка
                let (idx1, cpu1) = cpus[i];
                let usage1 = cpu1.cpu_usage();
                let (bar1, col1) = make_progress_bar(usage1, 10);
                write!(out, "  Core #{:<2}: ", idx1)?;
                execute!(out, SetForegroundColor(col1))?;
                write!(out, "{} {:5.1}%   ", bar1, usage1)?;
                execute!(out, ResetColor)?;

                // Права колонка
                if i + half < cpus.len() {
                    let (idx2, cpu2) = cpus[i + half];
                    let usage2 = cpu2.cpu_usage();
                    let (bar2, col2) = make_progress_bar(usage2, 10);
                    write!(out, "Core #{:<2}: ", idx2)?;
                    execute!(out, SetForegroundColor(col2))?;
                    writeln!(out, "{} {:5.1}%", bar2, usage2)?;
                    execute!(out, ResetColor)?;
                } else {
                    writeln!(out)?;
                }
            }

            writeln!(out, "--------------------------------------------------------------------------------")?;

            // 3. RAM Usage
            let total_ram = sys.total_memory();
            let used_ram = sys.used_memory();
            let ram_percentage = if total_ram > 0 {
                (used_ram as f32 / total_ram as f32) * 100.0
            } else {
                0.0
            };
            let (ram_bar, ram_color) = make_progress_bar(ram_percentage, 25);

            execute!(out, SetForegroundColor(Color::White))?;
            write!(out, " RAM Usage:       ")?;
            execute!(out, SetForegroundColor(ram_color))?;
            writeln!(
                out,
                "{} {:5.1}% ({}/ {})",
                ram_bar,
                ram_percentage,
                format_bytes(used_ram),
                format_bytes(total_ram)
            )?;
            execute!(out, ResetColor)?;

            // 4. Swap Usage
            let total_swap = sys.total_swap();
            let used_swap = sys.used_swap();
            let swap_percentage = if total_swap > 0 {
                (used_swap as f32 / total_swap as f32) * 100.0
            } else {
                0.0
            };
            let (swap_bar, swap_color) = make_progress_bar(swap_percentage, 25);

            execute!(out, SetForegroundColor(Color::White))?;
            write!(out, " Swap Usage:      ")?;
            execute!(out, SetForegroundColor(swap_color))?;
            writeln!(
                out,
                "{} {:5.1}% ({}/ {})",
                swap_bar,
                swap_percentage,
                format_bytes(used_swap),
                format_bytes(total_swap)
            )?;
            execute!(out, ResetColor)?;

            writeln!(out, "================================================================================")?;
            out.flush()?;
        }
    }

    // Відновлення термінала при виході
    execute!(out, Clear(ClearType::All), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    println!("Монітор успішно завершив роботу.");
    Ok(())
}
