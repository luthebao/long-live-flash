use crate::player::{PlayerBuilder, PlayerMode, PlayerRuntime};
use crate::tag_utils::SwfMovie;
use llflash_common::duration::FloatDuration;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub type WorkerId = u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum WorkerExecutionState {
    New = 0,
    Running = 1,
    Terminated = 2,
}

impl WorkerExecutionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Running => "running",
            Self::Terminated => "terminated",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Running,
            2 => Self::Terminated,
            _ => Self::New,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageChannelExecutionState {
    Open = 0,
    Closing = 1,
    Closed = 2,
}

impl MessageChannelExecutionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closing => "closing",
            Self::Closed => "closed",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Closing,
            2 => Self::Closed,
            _ => Self::Open,
        }
    }
}

#[derive(Clone)]
pub enum WorkerValue {
    Serialized(Vec<u8>),
    MessageChannel(Arc<MessageChannelHandle>),
}

pub struct MessageChannelHandle {
    sender: WorkerId,
    receiver: WorkerId,
    state: AtomicU8,
    sequence: AtomicU64,
    queue: Mutex<VecDeque<WorkerValue>>,
    queue_changed: Condvar,
}

impl MessageChannelHandle {
    pub fn new(sender: WorkerId, receiver: WorkerId) -> Arc<Self> {
        Arc::new(Self {
            sender,
            receiver,
            state: AtomicU8::new(MessageChannelExecutionState::Open as u8),
            sequence: AtomicU64::new(0),
            queue: Mutex::new(VecDeque::new()),
            queue_changed: Condvar::new(),
        })
    }

    pub fn sender(&self) -> WorkerId {
        self.sender
    }

    pub fn receiver(&self) -> WorkerId {
        self.receiver
    }

    pub fn state(&self) -> MessageChannelExecutionState {
        MessageChannelExecutionState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    pub fn message_available(&self) -> bool {
        !self.queue.lock().unwrap().is_empty()
    }

    pub fn send(&self, value: WorkerValue, queue_limit: i32) -> Result<(), WorkerChannelError> {
        if self.state() != MessageChannelExecutionState::Open {
            return Err(WorkerChannelError::Closed);
        }

        let mut queue = self.queue.lock().unwrap();
        if queue_limit >= 0 && queue.len() >= queue_limit as usize {
            return Err(WorkerChannelError::QueueFull);
        }

        queue.push_back(value);
        self.sequence.fetch_add(1, Ordering::AcqRel);
        drop(queue);
        self.queue_changed.notify_all();
        Ok(())
    }

    pub fn receive(&self, block: bool) -> Result<Option<WorkerValue>, WorkerChannelError> {
        let mut queue = self.queue.lock().unwrap();

        loop {
            if let Some(value) = queue.pop_front() {
                if queue.is_empty() && self.state() == MessageChannelExecutionState::Closing {
                    self.state
                        .store(MessageChannelExecutionState::Closed as u8, Ordering::Release);
                    self.queue_changed.notify_all();
                }
                return Ok(Some(value));
            }

            match self.state() {
                MessageChannelExecutionState::Closed | MessageChannelExecutionState::Closing => {
                    self.state
                        .store(MessageChannelExecutionState::Closed as u8, Ordering::Release);
                    return Err(WorkerChannelError::Closed);
                }
                MessageChannelExecutionState::Open if !block => return Ok(None),
                MessageChannelExecutionState::Open => {
                    queue = self.queue_changed.wait(queue).unwrap();
                }
            }
        }
    }

    pub fn close(&self) {
        let mut queue = self.queue.lock().unwrap();
        if self.state() == MessageChannelExecutionState::Closed {
            return;
        }

        if queue.is_empty() {
            self.state
                .store(MessageChannelExecutionState::Closed as u8, Ordering::Release);
        } else {
            self.state
                .store(MessageChannelExecutionState::Closing as u8, Ordering::Release);
        }
        queue.clear();
        self.state
            .store(MessageChannelExecutionState::Closed as u8, Ordering::Release);
        drop(queue);
        self.queue_changed.notify_all();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerChannelError {
    Closed,
    QueueFull,
}

pub struct WorkerHandle {
    id: WorkerId,
    primordial: bool,
    state: AtomicU8,
    terminate_requested: AtomicBool,
    swf: Mutex<Option<Vec<u8>>>,
    shared_properties: Mutex<HashMap<String, WorkerValue>>,
}

impl WorkerHandle {
    fn primordial(id: WorkerId) -> Arc<Self> {
        Arc::new(Self {
            id,
            primordial: true,
            state: AtomicU8::new(WorkerExecutionState::Running as u8),
            terminate_requested: AtomicBool::new(false),
            swf: Mutex::new(None),
            shared_properties: Mutex::new(HashMap::new()),
        })
    }

    fn background(id: WorkerId, swf: Vec<u8>) -> Arc<Self> {
        Arc::new(Self {
            id,
            primordial: false,
            state: AtomicU8::new(WorkerExecutionState::New as u8),
            terminate_requested: AtomicBool::new(false),
            swf: Mutex::new(Some(swf)),
            shared_properties: Mutex::new(HashMap::new()),
        })
    }

    pub fn id(&self) -> WorkerId {
        self.id
    }

    pub fn is_primordial(&self) -> bool {
        self.primordial
    }

    pub fn state(&self) -> WorkerExecutionState {
        WorkerExecutionState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn termination_requested(&self) -> bool {
        self.terminate_requested.load(Ordering::Acquire)
    }

    pub fn set_shared_property(&self, key: String, value: WorkerValue) {
        self.shared_properties.lock().unwrap().insert(key, value);
    }

    pub fn get_shared_property(&self, key: &str) -> Option<WorkerValue> {
        self.shared_properties.lock().unwrap().get(key).cloned()
    }

    pub fn terminate(&self) -> bool {
        if self.primordial || self.state() == WorkerExecutionState::New {
            return false;
        }

        if self.state() == WorkerExecutionState::Terminated {
            return false;
        }

        self.terminate_requested.store(true, Ordering::Release);
        true
    }

    fn take_swf(&self) -> Option<Vec<u8>> {
        self.swf.lock().unwrap().take()
    }

    fn set_state(&self, state: WorkerExecutionState) {
        self.state.store(state as u8, Ordering::Release);
    }
}

pub struct WorkerDomainHandle {
    next_id: AtomicU64,
    workers: Mutex<Vec<Arc<WorkerHandle>>>,
}

impl WorkerDomainHandle {
    pub fn new() -> (Arc<Self>, Arc<WorkerHandle>) {
        let primordial = WorkerHandle::primordial(0);
        let domain = Arc::new(Self {
            next_id: AtomicU64::new(1),
            workers: Mutex::new(vec![primordial.clone()]),
        });
        (domain, primordial)
    }

    pub fn create_worker(&self, swf: Vec<u8>) -> Arc<WorkerHandle> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let worker = WorkerHandle::background(id, swf);
        self.workers.lock().unwrap().push(worker.clone());
        worker
    }

    pub fn running_workers(&self) -> Vec<Arc<WorkerHandle>> {
        self.workers
            .lock()
            .unwrap()
            .iter()
            .filter(|worker| worker.state() == WorkerExecutionState::Running)
            .cloned()
            .collect()
    }
}

#[derive(Clone)]
pub struct WorkerRuntimeContext {
    domain: Arc<WorkerDomainHandle>,
    current: Arc<WorkerHandle>,
}

impl WorkerRuntimeContext {
    pub fn primordial() -> Self {
        let (domain, current) = WorkerDomainHandle::new();
        Self { domain, current }
    }

    pub fn background(domain: Arc<WorkerDomainHandle>, current: Arc<WorkerHandle>) -> Self {
        Self { domain, current }
    }

    pub fn domain(&self) -> Arc<WorkerDomainHandle> {
        self.domain.clone()
    }

    pub fn current(&self) -> Arc<WorkerHandle> {
        self.current.clone()
    }
}

impl Default for WorkerRuntimeContext {
    fn default() -> Self {
        Self::primordial()
    }
}

#[derive(Clone, Copy)]
pub struct WorkerLaunchConfig {
    pub player_version: u8,
    pub player_runtime: PlayerRuntime,
    pub player_mode: PlayerMode,
}

pub fn is_supported() -> bool {
    cfg!(not(target_family = "wasm"))
}

pub fn start_worker(
    domain: Arc<WorkerDomainHandle>,
    worker: Arc<WorkerHandle>,
    config: WorkerLaunchConfig,
) -> bool {
    if !is_supported() || worker.is_primordial() {
        return false;
    }

    if worker
        .state
        .compare_exchange(
            WorkerExecutionState::New as u8,
            WorkerExecutionState::Running as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return false;
    }

    let Some(swf_bytes) = worker.take_swf() else {
        worker.set_state(WorkerExecutionState::Terminated);
        return false;
    };

    #[cfg(not(target_family = "wasm"))]
    {
        let thread_worker = worker.clone();
        let thread_domain = domain.clone();
        let name = format!("llflash-avm2-worker-{}", worker.id());
        let spawn_result = thread::Builder::new().name(name).spawn(move || {
            run_worker_thread(thread_domain, thread_worker.clone(), swf_bytes, config);
            thread_worker.set_state(WorkerExecutionState::Terminated);
        });

        if spawn_result.is_err() {
            worker.set_state(WorkerExecutionState::Terminated);
            return false;
        }

        true
    }

    #[cfg(target_family = "wasm")]
    {
        let _ = (domain, worker, swf_bytes, config);
        false
    }
}

#[cfg(not(target_family = "wasm"))]
fn run_worker_thread(
    domain: Arc<WorkerDomainHandle>,
    worker: Arc<WorkerHandle>,
    swf_bytes: Vec<u8>,
    config: WorkerLaunchConfig,
) {
    let url = format!("worker://{}.swf", worker.id());
    let movie = match SwfMovie::from_data(&swf_bytes, url, None) {
        Ok(movie) => movie,
        Err(error) => {
            tracing::error!(worker_id = worker.id(), ?error, "Unable to parse worker SWF");
            return;
        }
    };

    let runtime = WorkerRuntimeContext::background(domain, worker.clone());
    let player = PlayerBuilder::new()
        .with_movie(movie)
        .with_autoplay(true)
        .with_player_version(Some(config.player_version))
        .with_player_runtime(config.player_runtime)
        .with_player_mode(config.player_mode)
        .with_worker_runtime_context(runtime)
        .build();

    let mut last_tick = Instant::now();
    while !worker.termination_requested() {
        let now = Instant::now();
        let dt = now.duration_since(last_tick);
        last_tick = now;

        let sleep_for = {
            let mut player = player.lock().unwrap();
            player.tick(FloatDuration::from_secs(dt.as_secs_f64()));
            player.time_til_next_frame().min(Duration::from_millis(10))
        };

        if worker.termination_requested() {
            break;
        }

        if !sleep_for.is_zero() {
            thread::sleep(sleep_for);
        } else {
            thread::yield_now();
        }
    }
}
