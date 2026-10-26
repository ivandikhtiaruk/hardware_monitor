use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, 
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use std::{
    io::{stdout, Write},
    thread,
    time::{Duration, Instant},
};
use sysinfo::System;

// Системна структура ядра Windows для точного зняття тактових імпульсів CPU
#[repr(C)]
#[derive(Default, Copy, Clone)]
struct SystemProcessorPerformanceInformation {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

type NtQuerySystemInformationFn = unsafe extern "system" fn(
    u32,
    *mut std::ffi::c_void,
    u32,
    *mut u32,
) -> i32;

extern "system" {
    fn GetModuleHandleA(lp_module_name: *const u8) -> *mut std::ffi::c_void;
    fn GetProcAddress(h_module: *mut std::ffi::c_void, lp_proc_name: *const u8) -> *mut std::ffi::c_void;
}

struct WinCpuTracker {
    query_fn: Option<NtQuerySystemInformationFn>,
    prev_times: Vec<SystemProcessorPerformanceInformation>,
    num_cores: usize,
}

impl WinCpuTracker {
    fn new(num_cores: usize) -> Self {
        let query_fn = unsafe {
            let ntdll = GetModuleHandleA(b"ntdll.dll\0".as_ptr());
            if ntdll.is_null() {
                None
            } else {
                let proc = GetProcAddress(ntdll, b"NtQuerySystemInformation\0".as_ptr());
                if proc.is_null() {
                    None
                } else {
                    Some(std::mem::transmute(proc))
                }
            }
        };

        let mut tracker = Self {
            query_fn,
            prev_times: vec![SystemProcessorPerformanceInformation::default(); num_cores],
            num_cores,
        };

        let mut init_times = vec![SystemProcessorPerformanceInformation::default(); num_cores];
        if tracker.fetch_raw(&mut init_times) {
            tracker.prev_times = init_times;
        }
        tracker
    }

    fn fetch_raw(&mut self, dest: &mut [SystemProcessorPerformanceInformation]) -> bool {
        if let Some(q_fn) = self.query_fn {
            let mut ret_len = 0u32;
            let status = unsafe {
                q_fn(
                    8, // SystemProcessorPerformanceInformation
                    dest.as_mut_ptr() as *mut std::ffi::c_void,
                    (dest.len() * std::mem::size_of::<SystemProcessorPerformanceInformation>()) as u32,
                    &mut ret_len,
                )
            };
            status >= 0
        } else {
            false
        }
    }

    fn get_usages(&mut self) -> (f32, Vec<f32>) {
        let mut curr_times = vec![SystemProcessorPerformanceInformation::default(); self.num_cores];
        if !self.fetch_raw(&mut curr_times) {
            return (0.0, vec![0.0; self.num_cores]);
        }

        let mut core_usages = Vec::with_capacity(self.num_cores);
        for i in 0..self.num_cores {
            let prev = self.prev_times[i];
            let curr = curr_times[i];

            let idle = (curr.idle_time - prev.idle_time) as f64;
            let kernel = (curr.kernel_time - prev.kernel_time) as f64;
            let user = (curr.user_time - prev.user_time) as f64;

            // У Windows KernelTime вже включає IdleTime
            let total = kernel + user;
            let usage = if total > 0.0 {
                let busy = total - idle;
                ((busy / total) * 100.0).clamp(0.0, 100.0) as f32
            } else {
                0.0
            };
            core_usages.push(usage);
        }

        self.prev_times = curr_times;

        let global_cpu = if core_usages.is_empty() {
            0.0
        } else {
            core_usages.iter().sum::<f32>() / core_usages.len() as f32
        };

        (global_cpu, core_usages)
    }
}

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
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, Hide, Clear(ClearType::All))?;

    let mut sys = System::new_all();
    sys.refresh_memory();

    let num_cores = sys.cpus().len().max(1);
    let mut cpu_tracker = WinCpuTracker::new(num_cores);

    // Пауза для розрахунку первинної дельти
    thread::sleep(Duration::from_millis(300));

    let refresh_rate = Duration::from_millis(1000);
    let mut last_tick = Instant::now();

    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= refresh_rate {
            // Отримуємо реальне навантаження напряму від ядра Windows
            let (global_cpu, core_usages) = cpu_tracker.get_usages();
            sys.refresh_memory();
            last_tick = Instant::now();

            execute!(out, MoveTo(0, 0), Clear(ClearType::All))?;

            execute!(out, SetForegroundColor(Color::Cyan))?;
            writeln!(out, "================================================================================")?;
            writeln!(out, "             HARDWARE RESOURCE MONITOR (Variant 14 - KERNEL DIRECT)             ")?;
            writeln!(out, "                    Press 'q' or 'Esc' to exit the monitor                      ")?;
            writeln!(out, "================================================================================")?;
            execute!(out, ResetColor)?;

            // 1. Загальне навантаження CPU
            let (cpu_bar, cpu_color) = make_progress_bar(global_cpu, 25);
            execute!(out, SetForegroundColor(Color::White))?;
            write!(out, " Total CPU Load:  ")?;
            execute!(out, SetForegroundColor(cpu_color))?;
            writeln!(out, "{} {:5.1}%", cpu_bar, global_cpu)?;
            execute!(out, ResetColor)?;

            // 2. Всі ядра у 2 колонки
            writeln!(out, " Per Core Breakdown (2 columns):")?;
            let half = (core_usages.len() + 1) / 2;

            for i in 0..half {
                let usage1 = core_usages[i];
                let (bar1, col1) = make_progress_bar(usage1, 10);
                write!(out, "  Core #{:<2}: ", i)?;
                execute!(out, SetForegroundColor(col1))?;
                write!(out, "{} {:5.1}%   ", bar1, usage1)?;
                execute!(out, ResetColor)?;

                if i + half < core_usages.len() {
                    let idx2 = i + half;
                    let usage2 = core_usages[idx2];
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

            // 3. RAM
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

            // 4. Swap
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

    execute!(out, Clear(ClearType::All), Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    println!("Монітор успішно завершив роботу.");
    Ok(())
}
