// Experiment for tauri-apps/tao#1338. Not for merging.
// REPRO_MODE=tao     minimize/restore with set_minimized
// REPRO_MODE=syscmd  minimize/restore with WM_SYSCOMMAND
// Logs every WM_WINDOWPOSCHANGED / WM_SIZE of the window and counts "flashes":
// a non-iconic, non-maximized rect seen between asking to minimize and becoming iconic.

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn main() {
  use std::time::{Duration, Instant};
  use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoop},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::WindowBuilder,
  };
  use windows::Win32::{
    Foundation::*, System::Threading::GetCurrentThreadId, UI::WindowsAndMessaging::*,
  };

  static mut TARGET: isize = 0;
  static mut PHASE: u8 = 0; // 1 = between minimize request and iconic
  static mut FLASHES: u32 = 0;
  static mut START: Option<Instant> = None;

  unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let cwp = &*(lparam.0 as *const CWPRETSTRUCT);
    if code >= 0 && cwp.hwnd.0 as isize == TARGET {
      let t = START.unwrap().elapsed().as_millis();
      let iconic = IsIconic(cwp.hwnd).as_bool();
      let zoomed = IsZoomed(cwp.hwnd).as_bool();
      match cwp.message {
        WM_WINDOWPOSCHANGED => {
          let wp = &*(cwp.lParam.0 as *const WINDOWPOS);
          println!(
            "{t:>6} POSCHANGED {}x{} at {},{} flags={:#x} iconic={iconic} zoomed={zoomed}",
            wp.cx, wp.cy, wp.x, wp.y, wp.flags.0
          );
          if PHASE == 1 && !iconic && !zoomed && wp.cx > 0 {
            println!("{t:>6} *** FLASH");
            FLASHES += 1;
          }
          if iconic {
            PHASE = 0;
          }
        }
        WM_SIZE => println!("{t:>6} WM_SIZE type={} iconic={iconic} zoomed={zoomed}", cwp.wParam.0),
        WM_SHOWWINDOW => println!("{t:>6} WM_SHOWWINDOW {}", cwp.wParam.0),
        WM_SYSCOMMAND => println!("{t:>6} WM_SYSCOMMAND {:#x}", cwp.wParam.0),
        _ => {}
      }
    }
    CallNextHookEx(None, code, wparam, lparam)
  }

  let mode = std::env::args().nth(1).or_else(|| std::env::var("REPRO_MODE").ok()).unwrap_or_else(|| "tao".into());
  println!("mode = {mode}");
  let event_loop = EventLoop::new();
  let window = WindowBuilder::new()
    .with_decorations(false)
    .with_visible(false)
    .with_inner_size(LogicalSize::new(800.0, 600.0))
    .with_undecorated_shadow(true)
    .build(&event_loop)
    .unwrap();
  let raw = window.hwnd();
  unsafe {
    START = Some(Instant::now());
    TARGET = window.hwnd();
    SetWindowsHookExW(WH_CALLWNDPROCRET, Some(hook), None, GetCurrentThreadId()).unwrap();
  }
  window.set_visible(true);

  let syscmd = move |cmd: u32| unsafe {
    SendMessageW(HWND(raw as _), WM_SYSCOMMAND, Some(WPARAM(cmd as usize)), Some(LPARAM(0)));
  };
  let mut step = 0u32;
  let cycles = 6;

  event_loop.run(move |event, _, control_flow| {
    if let Event::NewEvents(StartCause::Init | StartCause::ResumeTimeReached { .. }) = event {
      let t = unsafe { START.unwrap().elapsed().as_millis() };
      match step % 4 {
        0 => {
          println!("{t:>6} --- maximize (cycle {})", step / 4);
          window.set_maximized(true);
        }
        1 => {
          println!("{t:>6} --- minimize");
          unsafe { PHASE = 1 };
          if mode == "syscmd" {
            syscmd(SC_MINIMIZE);
          } else {
            window.set_minimized(true);
          }
        }
        2 => {
          println!("{t:>6} --- restore");
          if mode == "syscmd" {
            syscmd(SC_RESTORE);
          } else {
            window.set_minimized(false);
          }
        }
        _ => {
          println!("{t:>6} --- unmaximize");
          window.set_maximized(false);
        }
      }
      step += 1;
      if step == cycles * 4 {
        println!("RESULT mode={mode} shadow={} exp={:?} flashes={} of {cycles}", true, std::env::var("TAO_EXP").ok(), unsafe { FLASHES });
        *control_flow = ControlFlow::Exit;
        return;
      }
      *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(1500));
    }
  });
}
