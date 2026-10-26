use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::{
    io::{stdout, Write},
    thread,
    time::{Duration, Instant},
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// Функція генерації візуального прогрес-бару з кольоровою індикацією
fn make_progress_bar(percentage: f32, width: usize) -> (String, Color) {
    let clamped = percentage.clamp(0.0, 100.0);
    let filled = ((clamped / 100.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

    // Колірна індикація навантаження: Зелений -> Жовтий -> Червоний
    let color = if clamped < 50.0 {
        Color::Green
    } else if clamped < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    };

    (bar, color)
}

/// Конвертація байтів у зручні одиниці GiB
fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    format!("{:.2} GiB", bytes as f64 / GIB)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Налаштовуємо термінал у Raw Mode (без буферизації рядків)
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, Clear(ClearType::All), Hide)?;

    // 2. Ініціалізуємо sysinfo лише для потрібних компонентів (мінімізація навантаження)
    let refresh_kind = RefreshKind::new()
        .with_cpu(CpuRefreshKind::everything())
        .with_memory(MemoryRefreshKind::everything());
    let mut sys = System::new_with_specifics(refresh_kind);

    // Перший вимір (калібрування для розрахунку дельти часу процесора)
    sys.refresh_specifics(refresh_kind);
    thread::sleep(Duration::from_millis(200));

    let refresh_rate = Duration::from_millis(1000);
    let mut last_tick = Instant::now();

    // 3. Основний цикл моніторингу в реальному часі
    loop {
        // Неблокуюча перевірка натискання клавіш для виходу ('q' або Esc)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        // Періодичне оновлення даних кожну 1 секунду
        if last_tick.elapsed() >= refresh_rate {
            sys.refresh_specifics(refresh_kind);
            last_tick = Instant::now();

            // Переміщуємо курсор у початок (без мерехтіння)
            execute!(out, MoveTo(0, 0))?;

            // Заголовок інтерфейсу
            execute!(out, SetForegroundColor(Color::Cyan))?;
            writeln!(out, "==========================================================")?;
            writeln!(out, "           HARDWARE RESOURCE MONITOR (Variant 14)         ")?;
            writeln!(out, "            Press 'q' or 'Esc' to exit the monitor        ")?;
            writeln!(out, "==========================================================")?;
            execute!(out, ResetColor)?;

            // 1. CPU Usage
            let global_cpu = sys.global_cpu_info().cpu_usage();
            let (cpu_bar, cpu_color) = make_progress_bar(global_cpu, 25);

            execute!(out, SetForegroundColor(Color::White))?;
            write!(out, " Total CPU Load:  ")?;
            execute!(out, SetForegroundColor(cpu_color))?;
            writeln!(out, "{} {:5.1}%", cpu_bar, global_cpu)?;
            execute!(out, ResetColor)?;

            writeln!(out, " Per Core Breakdown:")?;
            for (idx, cpu) in sys.cpus().iter().enumerate() {
                let usage = cpu.cpu_usage();
                let (core_bar, core_color) = make_progress_bar(usage, 15);
                write!(out, "   Core #{:<2}: ", idx)?;
                execute!(out, SetForegroundColor(core_color))?;
                writeln!(out, "{} {:5.1}%", core_bar, usage)?;
                execute!(out, ResetColor)?;
            }

            writeln!(out, "----------------------------------------------------------")?;

            // 2. RAM Usage
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

            // 3. Swap Usage
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

            writeln!(out, "==========================================================")?;
            out.flush()?;
        }
    }

    // 4. Відновлення нормального стану термінала при виході
    execute!(out, Clear(ClearType::All), Show, MoveTo(0, 0))?;
    disable_raw_mode()?;
    println!("Монітор успішно завершив роботу.");
    Ok(())
}
