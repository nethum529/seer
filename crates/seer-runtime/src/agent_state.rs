use std::io;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use seer_core::proto::AgentState;

use crate::pane_output::PaneOutput;
use crate::pty::lock_mutex;

mod process;
mod rules;

const INTERVAL: Duration = Duration::from_millis(250);
const ACTIVITY_WINDOW: Duration = Duration::from_millis(750);

#[derive(Clone, Default)]
pub(crate) struct Observation {
    pub(crate) name: String,
    pub(crate) agent_state: Option<AgentState>,
}

pub(crate) struct Detector {
    observation: Arc<Mutex<Observation>>,
    stop: Option<mpsc::Sender<()>>,
}

impl Detector {
    pub(crate) fn start(
        descriptor: Option<OwnedFd>,
        shell_pid: Option<u32>,
        output: Weak<Mutex<PaneOutput>>,
    ) -> io::Result<Self> {
        let observation = Arc::new(Mutex::new(Observation::default()));
        let Some(descriptor) = descriptor else {
            return Ok(Self {
                observation,
                stop: None,
            });
        };
        let (stop, stopped) = mpsc::channel();
        let cache = Arc::clone(&observation);
        thread::Builder::new()
            .name("agent-state".into())
            .spawn(move || {
                let mut sample = Sample::new();
                loop {
                    sample.update(&descriptor, shell_pid, &output, &cache);
                    if stopped.recv_timeout(INTERVAL) != Err(mpsc::RecvTimeoutError::Timeout) {
                        break;
                    }
                }
            })?;
        Ok(Self {
            observation,
            stop: Some(stop),
        })
    }

    pub(crate) fn observation(&self) -> Observation {
        lock_mutex(&self.observation).clone()
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        self.stop.take();
    }
}

struct Sample {
    group: i32,
    agent: Option<rules::Agent>,
    revision: u64,
    active: Instant,
    title: String,
    title_current: bool,
}

impl Sample {
    fn new() -> Self {
        Self {
            group: 0,
            agent: None,
            revision: 0,
            active: Instant::now(),
            title: String::new(),
            title_current: false,
        }
    }

    fn update(
        &mut self,
        descriptor: &OwnedFd,
        shell_pid: Option<u32>,
        output: &Weak<Mutex<PaneOutput>>,
        cache: &Mutex<Observation>,
    ) {
        let job = process::observe(descriptor, shell_pid);
        let changed = job.group != self.group || job.agent != self.agent;
        if changed {
            self.group = job.group;
            self.agent = job.agent;
            self.title_current = false;
        }
        let Some(agent) = job.agent else {
            *lock_mutex(cache) = Observation {
                name: job.name,
                agent_state: None,
            };
            return;
        };
        if changed {
            *lock_mutex(cache) = Observation {
                name: agent.name().into(),
                agent_state: None,
            };
        }
        let Some(output) = output.upgrade() else {
            return;
        };
        let (screen, title, revision) = {
            let Ok(pane) = output.try_lock() else {
                return;
            };
            (pane.grid.agent_screen(), pane.grid.title(), pane.revision)
        };
        if revision != self.revision {
            self.revision = revision;
            self.active = Instant::now();
        }
        if title != self.title {
            self.title.clone_from(&title);
            self.title_current = !changed;
        }
        let title = if self.title_current {
            title.as_str()
        } else {
            ""
        };
        let state = agent.classify(&screen, title).unwrap_or_else(|| {
            if self.active.elapsed() < ACTIVITY_WINDOW {
                AgentState::Working
            } else {
                AgentState::Idle
            }
        });
        *lock_mutex(cache) = Observation {
            name: agent.name().into(),
            agent_state: Some(state),
        };
    }
}
