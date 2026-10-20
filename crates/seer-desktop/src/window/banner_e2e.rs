use std::{
    net::{Shutdown, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use futures::StreamExt;
use gpui::{TestAppContext, VisualTestContext, px, size};
use seer_core::{
    Tree,
    proto::{ClientMsg, ServerMsg, codec},
};

use crate::window::{
    Content,
    input_e2e::{Runtime, open_view},
};

#[gpui::test]
fn room_drop_keeps_local_input_and_recovers_without_action(cx: &mut TestAppContext) {
    let room = Room::start();
    let runtime = Runtime::start();
    let (view, cx, listener) = join(&room, &runtime, cx);
    for width in [960., 680., 320.] {
        cx.simulate_resize(size(px(width), px(640.)));
        cx.run_until_parked();
        let normal_top = cx.debug_bounds("terminal").unwrap().top();
        let center = cx.debug_bounds("terminal").unwrap().center();
        cx.simulate_click(center, gpui::Modifiers::default());
        let dropped = Instant::now();
        room.online.store(false, Ordering::Release);
        wait(cx, || !room.connected.load(Ordering::Acquire));
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            cx.run_until_parked();
            if cx.debug_bounds("terminal").unwrap().top() > normal_top {
                break;
            }
            assert!(Instant::now() < deadline, "a lost room must show a banner");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(dropped.elapsed() < Duration::from_secs(2));
        println!(
            "width {width}: banner appeared after {:?}",
            dropped.elapsed()
        );
        let banner = cx.debug_bounds("status-banner").unwrap();
        let terminal = cx.debug_bounds("terminal").unwrap();
        assert!(
            banner.bottom() <= terminal.top(),
            "banner must reserve space"
        );
        assert!(banner.left() >= px(0.) && banner.right() <= px(width));
        let previous = runtime.input().len();
        cx.simulate_keystrokes("k");
        wait(cx, || runtime.input().len() == previous + 1);
        assert!(runtime.input().ends_with(b"k"));
        room.online.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            if cx.debug_bounds("terminal").unwrap().top() == normal_top {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "return must clear the banner: lost={}, connected={}",
                view.read_with(cx, |view, _| view.room.lost),
                room.connected.load(Ordering::Acquire)
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    drop(listener);
}

#[gpui::test]
fn room_stop_from_another_window_shows_stopped_state(cx: &mut TestAppContext) {
    for host in [false, true] {
        let room = Room::start();
        let runtime = Runtime::start();
        let (view, window_cx, listener) = join(&room, &runtime, cx);
        view.update(window_cx, |view, _| {
            view.room.current.as_mut().unwrap().host = host;
        });
        room.stop.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            window_cx.run_until_parked();
            if view.read_with(window_cx, |view, _| {
                matches!(&view.content, Content::Message(text) if text.as_ref() == if host { super::STOPPED } else { "The host stopped the room" })
            }) && window_cx.debug_bounds("stopped-room").is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "a broker stop must show the stopped room screen without a banner, host={host}"
            );
            thread::sleep(Duration::from_millis(5));
        }
        drop(listener);
    }
}

fn join<'a>(
    room: &Room,
    runtime: &Runtime,
    cx: &'a mut TestAppContext,
) -> (
    gpui::Entity<super::SeerWindow>,
    &'a mut VisualTestContext,
    gpui::Task<()>,
) {
    let (view, cx) = open_view(runtime, cx);
    let mut messages = view.update(cx, |view, _| {
        let Content::Terminal(link, _) = &mut view.content else {
            panic!("terminal missing")
        };
        link.join_room(room.server.clone(), None)
    });
    let listener = cx.spawn({
        let view = view.clone();
        async move |mut cx| {
            while let Some(event) = messages.next().await {
                view.update(&mut cx, |view, cx| view.receive(event, cx))
                    .unwrap();
            }
        }
    });
    wait(cx, || room.connected.load(Ordering::Acquire));
    (view, cx, listener)
}

fn wait(cx: &mut VisualTestContext, ready: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if ready() {
            return;
        }
        assert!(Instant::now() < deadline, "room or shell did not respond");
        thread::sleep(Duration::from_millis(5));
    }
}

struct Room {
    server: seer::ServerEntry,
    online: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Room {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = serde_json::from_value(serde_json::json!({"endpoint":listener.local_addr().unwrap().to_string(),"alias":"E2e","user_id":"e2e","name":"E2e","credential":"e2e","current":true})).unwrap();
        let online = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let (up, joined, stopped) = (online.clone(), connected.clone(), done.clone());
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                let Ok((mut stream, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                };
                if !up.load(Ordering::Acquire) {
                    continue;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                if !matches!(codec::decode(&mut stream), Ok(ClientMsg::Hello { .. })) {
                    continue;
                }
                if codec::encode(
                    &mut stream,
                    &ServerMsg::Welcome {
                        user_id: "e2e".into(),
                        name: "E2e".into(),
                        client_id: "banner".into(),
                        tree: Tree::new(),
                    },
                )
                .is_err()
                {
                    continue;
                }
                joined.store(true, Ordering::Release);
                while up.load(Ordering::Acquire)
                    && !stopped.load(Ordering::Acquire)
                    && !stopping.load(Ordering::Acquire)
                {
                    thread::sleep(Duration::from_millis(5));
                }
                if stopping.load(Ordering::Acquire) {
                    codec::encode(
                        &mut stream,
                        &ServerMsg::Bye {
                            reason: "server stopped".into(),
                        },
                    )
                    .unwrap();
                    let _ = stream.shutdown(Shutdown::Both);
                    joined.store(false, Ordering::Release);
                    break;
                }
                let _ = stream.shutdown(Shutdown::Both);
                joined.store(false, Ordering::Release);
            }
        });
        Self {
            server,
            online,
            connected,
            done,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Room {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        self.worker.take().unwrap().join().unwrap();
    }
}
