#[derive(Debug, Default)]
pub struct GenerationGate {
    current: u64,
}

#[derive(Debug, Default)]
pub struct SingleFlightGate {
    running: bool,
}

#[derive(Debug)]
pub(super) struct TaskSlot<P> {
    pub(super) generation: GenerationGate,
    pub(super) single_flight: SingleFlightGate,
    pub(super) pending_latest: Option<(u64, P)>,
    // The in-flight generation is distinct from `generation.current()`:
    // invalidating or superseding a request never cancels a running worker.
    // An old/duplicate completion must not release the next worker's slot.
    running_generation: Option<u64>,
}

#[derive(Debug)]
pub(super) enum LatestRequest<P> {
    Started { generation: u64, request: P },
    Queued { generation: u64 },
}

#[derive(Debug)]
pub(super) enum LatestCompletion<P> {
    Restart { generation: u64, request: P },
    Deliver(bool),
}

impl<P> Default for TaskSlot<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> TaskSlot<P> {
    pub(super) const fn new() -> Self {
        Self {
            generation: GenerationGate::new(),
            single_flight: SingleFlightGate::new(),
            pending_latest: None,
            running_generation: None,
        }
    }

    pub(super) fn invalidate_pending(&mut self) {
        self.pending_latest = None;
        self.generation.begin();
    }

    pub(super) fn try_begin(&mut self) -> Option<u64> {
        if self.single_flight.try_start() {
            let generation = self.generation.begin();
            self.running_generation = Some(generation);
            Some(generation)
        } else {
            None
        }
    }

    pub(super) fn request_latest(&mut self, request: P) -> LatestRequest<P> {
        let generation = self.generation.begin();
        if self.single_flight.try_start() {
            self.running_generation = Some(generation);
            LatestRequest::Started {
                generation,
                request,
            }
        } else {
            self.pending_latest = Some((generation, request));
            LatestRequest::Queued { generation }
        }
    }

    pub(super) fn finish(&mut self, generation: u64) -> bool {
        // In particular, do not release an active new worker when an old
        // generation reports a duplicate or delayed completion.
        if self.running_generation != Some(generation) {
            return false;
        }
        self.running_generation = None;
        self.single_flight.finish();
        self.generation.is_current(generation)
    }

    pub(super) fn finish_latest(&mut self, generation: u64) -> LatestCompletion<P> {
        if self.running_generation != Some(generation) {
            return LatestCompletion::Deliver(false);
        }
        self.running_generation = None;
        self.single_flight.finish();
        if let Some((next_generation, request)) = self.pending_latest.take() {
            let started = self.single_flight.try_start();
            debug_assert!(started);
            self.running_generation = Some(next_generation);
            LatestCompletion::Restart {
                generation: next_generation,
                request,
            }
        } else {
            LatestCompletion::Deliver(self.generation.is_current(generation))
        }
    }
}

impl SingleFlightGate {
    pub const fn new() -> Self {
        Self { running: false }
    }

    pub fn try_start(&mut self) -> bool {
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    pub fn finish(&mut self) {
        self.running = false;
    }

    pub const fn is_running(&self) -> bool {
        self.running
    }
}

impl GenerationGate {
    pub const fn new() -> Self {
        Self { current: 0 }
    }

    pub fn begin(&mut self) -> u64 {
        self.current = self.current.wrapping_add(1);
        if self.current == 0 {
            self.current = 1;
        }
        self.current
    }

    pub const fn is_current(&self, generation: u64) -> bool {
        generation == self.current
    }

    pub const fn current(&self) -> u64 {
        self.current
    }
}
