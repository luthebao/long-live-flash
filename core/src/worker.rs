use crate::player::{PlayerBuilder, PlayerMode, PlayerRuntime};
use crate::tag_utils::SwfMovie;
use llflash_common::duration::FloatDuration;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub type WorkerId = u64;
pub type MessageChannelId = u64;

static NEXT_MESSAGE_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

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
    Worker(Arc<WorkerHandle>),
    MessageChannel(Arc<MessageChannelHandle>),
}

pub struct MessageChannelHandle {
    id: MessageChannelId,
    sender: WorkerId,
    receiver: WorkerId,
    state: AtomicU8,
    state_sequence: AtomicU64,
    sequence: AtomicU64,
    queue: Mutex<VecDeque<WorkerValue>>,
    queue_changed: Condvar,
}

impl MessageChannelHandle {
    pub fn new(sender: WorkerId, receiver: WorkerId) -> Arc<Self> {
        Arc::new(Self {
            id: NEXT_MESSAGE_CHANNEL_ID.fetch_add(1, Ordering::Relaxed),
            sender,
            receiver,
            state: AtomicU8::new(MessageChannelExecutionState::Open as u8),
            state_sequence: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            queue: Mutex::new(VecDeque::new()),
            queue_changed: Condvar::new(),
        })
    }

    pub fn id(&self) -> MessageChannelId {
        self.id
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

    pub fn state_sequence(&self) -> u64 {
        self.state_sequence.load(Ordering::Acquire)
    }

    fn set_state(&self, state: MessageChannelExecutionState) {
        let previous = self.state.swap(state as u8, Ordering::AcqRel);
        if previous != state as u8 {
            self.state_sequence.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn message_available(&self) -> bool {
        !self.queue.lock().unwrap().is_empty()
    }

    pub fn send(&self, value: WorkerValue, queue_limit: i32) -> Result<(), WorkerChannelError> {
        if self.state() != MessageChannelExecutionState::Open {
            return Err(WorkerChannelError::Closed);
        }

        let mut queue = self.queue.lock().unwrap();
        while queue_limit >= 0 && queue.len() > queue_limit as usize {
            if self.state() != MessageChannelExecutionState::Open {
                return Err(WorkerChannelError::Closed);
            }
            queue = self.queue_changed.wait(queue).unwrap();
        }

        if self.state() != MessageChannelExecutionState::Open {
            return Err(WorkerChannelError::Closed);
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
                    self.set_state(MessageChannelExecutionState::Closed);
                }
                self.queue_changed.notify_all();
                return Ok(Some(value));
            }

            match self.state() {
                MessageChannelExecutionState::Closed => {
                    return Err(WorkerChannelError::Closed);
                }
                MessageChannelExecutionState::Closing => {
                    self.set_state(MessageChannelExecutionState::Closed);
                    return Ok(None);
                }
                MessageChannelExecutionState::Open if !block => return Ok(None),
                MessageChannelExecutionState::Open => {
                    queue = self.queue_changed.wait(queue).unwrap();
                }
            }
        }
    }

    pub fn close(&self) {
        let queue = self.queue.lock().unwrap();
        if self.state() == MessageChannelExecutionState::Closed {
            return;
        }

        if queue.is_empty() {
            self.set_state(MessageChannelExecutionState::Closed);
        } else {
            self.set_state(MessageChannelExecutionState::Closing);
        }
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
    started: AtomicBool,
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
            started: AtomicBool::new(true),
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
            started: AtomicBool::new(false),
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

    pub fn clear_shared_property(&self, key: &str) {
        self.shared_properties.lock().unwrap().remove(key);
    }

    pub fn terminate(&self) -> bool {
        if self.primordial || !self.started.load(Ordering::Acquire) {
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
    enabled: bool,
}

impl WorkerRuntimeContext {
    pub fn primordial(enabled: bool) -> Self {
        let (domain, current) = WorkerDomainHandle::new();
        Self {
            domain,
            current,
            enabled: enabled && is_supported(),
        }
    }

    pub fn background(
        domain: Arc<WorkerDomainHandle>,
        current: Arc<WorkerHandle>,
        enabled: bool,
    ) -> Self {
        Self {
            domain,
            current,
            enabled: enabled && is_supported(),
        }
    }

    pub fn domain(&self) -> Arc<WorkerDomainHandle> {
        self.domain.clone()
    }

    pub fn current(&self) -> Arc<WorkerHandle> {
        self.current.clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for WorkerRuntimeContext {
    fn default() -> Self {
        Self::primordial(true)
    }
}

#[derive(Clone, Copy)]
pub struct WorkerLaunchConfig {
    pub player_version: u8,
    pub player_runtime: PlayerRuntime,
    pub player_mode: PlayerMode,
    pub worker_enabled: bool,
}

pub fn is_supported() -> bool {
    cfg!(not(target_family = "wasm"))
}

pub fn start_worker(
    domain: Arc<WorkerDomainHandle>,
    worker: Arc<WorkerHandle>,
    config: WorkerLaunchConfig,
) -> bool {
    if !config.worker_enabled || !is_supported() || worker.is_primordial() {
        return false;
    }

    if worker
        .started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
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

    let runtime = WorkerRuntimeContext::background(domain, worker.clone(), config.worker_enabled);
    let player = PlayerBuilder::new()
        .with_movie(movie)
        .with_autoplay(true)
        .with_player_version(Some(config.player_version))
        .with_player_runtime(config.player_runtime)
        .with_player_mode(config.player_mode)
        .with_worker_runtime_context(runtime)
        .build();

    if worker.termination_requested() {
        return;
    }
    worker.set_state(WorkerExecutionState::Running);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::log::LogBackend;
    use std::sync::mpsc;

    struct TestLog {
        traces: Arc<Mutex<Vec<String>>>,
    }

    impl LogBackend for TestLog {
        fn avm_trace(&self, message: &str) {
            self.traces.lock().unwrap().push(message.to_string());
        }

        fn avm_warning(&self, message: &str) {
            self.traces
                .lock()
                .unwrap()
                .push(format!("warning:{message}"));
        }
    }

    fn bytes(value: u8) -> WorkerValue {
        WorkerValue::Serialized(vec![value])
    }

    fn byte_value(value: WorkerValue) -> u8 {
        match value {
            WorkerValue::Serialized(bytes) => bytes[0],
            WorkerValue::Worker(_) | WorkerValue::MessageChannel(_) => {
                panic!("expected serialized test value")
            }
        }
    }

    #[test]
    fn worker_runtime_can_be_disabled() {
        let runtime = WorkerRuntimeContext::primordial(false);
        assert!(!runtime.is_enabled());

        let worker = runtime.domain().create_worker(vec![1, 2, 3]);
        let started = start_worker(
            runtime.domain(),
            worker.clone(),
            WorkerLaunchConfig {
                player_version: crate::DEFAULT_PLAYER_VERSION,
                player_runtime: PlayerRuntime::FlashPlayer,
                player_mode: PlayerMode::Release,
                worker_enabled: false,
            },
        );
        assert!(!started);
        assert_eq!(worker.state(), WorkerExecutionState::New);
    }

    #[test]
    fn worker_domain_tracks_primordial_and_background_workers() {
        let (domain, primordial) = WorkerDomainHandle::new();
        assert_eq!(primordial.id(), 0);
        assert!(primordial.is_primordial());
        assert_eq!(primordial.state(), WorkerExecutionState::Running);

        let worker = domain.create_worker(vec![1, 2, 3]);
        assert_eq!(worker.id(), 1);
        assert!(!worker.is_primordial());
        assert_eq!(worker.state(), WorkerExecutionState::New);
        assert_eq!(domain.running_workers().len(), 1);
    }

    #[test]
    fn shared_properties_are_shared_by_worker_handle() {
        let (domain, _) = WorkerDomainHandle::new();
        let worker = domain.create_worker(Vec::new());
        worker.set_shared_property("answer".into(), bytes(42));

        let value = worker.get_shared_property("answer").unwrap();
        assert_eq!(byte_value(value), 42);

        worker.clear_shared_property("answer");
        assert!(worker.get_shared_property("answer").is_none());
    }

    #[test]
    fn message_channel_is_fifo_and_tracks_sequence() {
        let channel = MessageChannelHandle::new(1, 2);
        assert_eq!(channel.state(), MessageChannelExecutionState::Open);
        assert_eq!(channel.sequence(), 0);
        assert!(!channel.message_available());

        channel.send(bytes(1), -1).unwrap();
        channel.send(bytes(2), -1).unwrap();
        assert_eq!(channel.sequence(), 2);
        assert!(channel.message_available());

        assert_eq!(byte_value(channel.receive(false).unwrap().unwrap()), 1);
        assert_eq!(byte_value(channel.receive(false).unwrap().unwrap()), 2);
        assert!(channel.receive(false).unwrap().is_none());
    }

    #[test]
    fn close_drains_queued_messages_before_closed() {
        let channel = MessageChannelHandle::new(1, 2);
        channel.send(bytes(7), -1).unwrap();
        channel.close();
        assert_eq!(channel.state(), MessageChannelExecutionState::Closing);

        assert_eq!(byte_value(channel.receive(false).unwrap().unwrap()), 7);
        assert_eq!(channel.state(), MessageChannelExecutionState::Closed);
        assert!(matches!(
            channel.receive(false),
            Err(WorkerChannelError::Closed)
        ));
    }

    #[test]
    fn queue_limit_blocks_until_receiver_drains() {
        let channel = MessageChannelHandle::new(1, 2);
        channel.send(bytes(1), 0).unwrap();
        let sender = channel.clone();
        let (done_tx, done_rx) = mpsc::channel();

        let thread = std::thread::spawn(move || {
            sender.send(bytes(2), 0).unwrap();
            done_tx.send(()).unwrap();
        });

        assert!(done_rx.recv_timeout(Duration::from_millis(25)).is_err());
        assert_eq!(byte_value(channel.receive(false).unwrap().unwrap()), 1);
        done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(byte_value(channel.receive(false).unwrap().unwrap()), 2);
        thread.join().unwrap();
    }

    fn worker_test_movie() -> SwfMovie {
        let mut bytes =
            include_bytes!("../../tests/tests/swfs/avm2/worker_basic/Test.swf").to_vec();
        assert_eq!(&bytes[0..3], b"FWS");
        bytes[3] = 17;
        SwfMovie::from_data(&bytes, "file:///worker_basic.swf".into(), None).unwrap()
    }

    #[test]
    fn avm2_worker_executes_on_background_runtime_and_returns_message() {
        let traces = Arc::new(Mutex::new(Vec::new()));
        let player = PlayerBuilder::new()
            .with_movie(worker_test_movie())
            .with_autoplay(true)
            .with_log(TestLog {
                traces: traces.clone(),
            })
            .build();

        for _ in 0..100 {
            player
                .lock()
                .unwrap()
                .tick(FloatDuration::from_millis(1000.0 / 30.0));
            if traces
                .lock()
                .unwrap()
                .iter()
                .any(|line| line == "result=42")
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let traces = traces.lock().unwrap();
        assert!(traces.iter().any(|line| line == "supported=true"));
        assert!(traces.iter().any(|line| line == "domainSupported=true"));
        assert!(traces.iter().any(|line| line == "primordial=true"));
        assert!(traces.iter().any(|line| line == "result=42"));
        assert!(traces.iter().any(|line| line == "label=worker-ok"));
        assert!(traces.iter().any(|line| line == "messageAvailable=false"));
        assert!(traces.iter().any(|line| line == "terminate=true"));
    }

    #[test]
    fn avm2_worker_disabled_is_visible_to_actionscript() {
        let traces = Arc::new(Mutex::new(Vec::new()));
        let player = PlayerBuilder::new()
            .with_movie(worker_test_movie())
            .with_autoplay(true)
            .with_worker_enabled(false)
            .with_log(TestLog {
                traces: traces.clone(),
            })
            .build();

        player
            .lock()
            .unwrap()
            .tick(FloatDuration::from_millis(1000.0 / 30.0));

        let traces = traces.lock().unwrap();
        assert!(traces.iter().any(|line| line == "supported=false"));
        assert!(traces.iter().any(|line| line == "domainSupported=false"));
        assert!(!traces.iter().any(|line| line.starts_with("result=")));
    }
}
