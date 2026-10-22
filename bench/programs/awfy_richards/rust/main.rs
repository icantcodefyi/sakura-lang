// Richards from Are We Fast Yet (benchmarks/JavaScript/richards.js), "a derivation from the SOM
// version, which is derived from Mario Wolczko's Smalltalk version"; see ../LICENSE.
// AWFY's objects live in two arenas owned by the scheduler (task control blocks indexed by
// identity, packets by creation order) and are mutated in place; an `Option<usize>` index is a
// reference and `None` is NO_TASK / NO_WORK, as the DeltaBlue ports use arena indices. Deviation:
// AWFY's task functions are closures made with their data record; here the data record is an
// enum and `run_task` matches on it, as every port of this program does. TRACING is false in
// AWFY; its trace code is left out. run(n) runs benchmark() n times and returns the number of
// true results.

const IDLER: usize = 0;
const WORKER: usize = 1;
const HANDLER_A: usize = 2;
const HANDLER_B: usize = 3;
const DEVICE_A: usize = 4;
const DEVICE_B: usize = 5;
const NUM_TYPES: usize = 6;

const DEVICE_PACKET_KIND: i32 = 0;
const WORK_PACKET_KIND: i32 = 1;

const DATA_SIZE: usize = 4;

type PacketRef = Option<usize>;
type TaskRef = Option<usize>;

struct Packet {
    link: PacketRef,
    identity: usize,
    kind: i32,
    datum: usize,
    data: [usize; DATA_SIZE],
}

#[derive(Clone, Copy)]
struct TaskState {
    packet_pending: bool,
    task_waiting: bool,
    task_holding: bool,
}

impl TaskState {
    fn create_running() -> TaskState {
        TaskState {
            packet_pending: false,
            task_waiting: false,
            task_holding: false,
        }
    }

    fn create_waiting() -> TaskState {
        TaskState {
            packet_pending: false,
            task_waiting: true,
            task_holding: false,
        }
    }

    fn create_waiting_with_packet() -> TaskState {
        TaskState {
            packet_pending: true,
            task_waiting: true,
            task_holding: false,
        }
    }
}

/// The task data records; the variant selects the task function.
#[derive(Clone, Copy)]
enum Data {
    Device {
        pending: PacketRef,
    },
    Handler {
        work_in: PacketRef,
        device_in: PacketRef,
    },
    Idle {
        control: i32,
        count: i32,
    },
    Worker {
        destination: usize,
        count: usize,
    },
}

struct TaskControlBlock {
    link: TaskRef,
    identity: usize,
    priority: i32,
    input: PacketRef,
    state: TaskState,
    handle: Data,
}

impl TaskControlBlock {
    fn is_task_holding_or_waiting(&self) -> bool {
        self.state.task_holding || (!self.state.packet_pending && self.state.task_waiting)
    }

    fn is_waiting_with_packet(&self) -> bool {
        self.state.packet_pending && self.state.task_waiting && !self.state.task_holding
    }
}

struct Scheduler {
    queue_packet_count: i32,
    hold_count: i32,
    task_table: Vec<Option<TaskControlBlock>>,
    packets: Vec<Packet>,
    task_list: TaskRef,
    current_task: TaskRef,
    current_task_identity: usize,
}

impl Scheduler {
    fn new() -> Scheduler {
        Scheduler {
            queue_packet_count: 0,
            hold_count: 0,
            task_table: (0..NUM_TYPES).map(|_| None).collect(),
            packets: Vec::new(),
            task_list: None,
            current_task: None,
            current_task_identity: 0,
        }
    }

    fn task(&mut self, id: usize) -> &mut TaskControlBlock {
        self.task_table[id].as_mut().expect("task")
    }

    fn current(&mut self) -> &mut TaskControlBlock {
        let id = self.current_task.expect("current task");
        self.task(id)
    }

    /// RBObject's append.
    fn append(&mut self, packet: usize, queue_head: PacketRef) -> PacketRef {
        self.packets[packet].link = None;
        let Some(head) = queue_head else {
            return Some(packet);
        };

        let mut mouse = head;
        while let Some(link) = self.packets[mouse].link {
            mouse = link;
        }
        self.packets[mouse].link = Some(packet);
        queue_head
    }

    fn add_input_and_check_priority(
        &mut self,
        t: usize,
        packet: usize,
        old_task: TaskRef,
    ) -> TaskRef {
        if self.task(t).input.is_none() {
            let task = self.task(t);
            task.input = Some(packet);
            task.state.packet_pending = true;
            let priority = task.priority;
            if priority > self.task(old_task.expect("old task")).priority {
                return Some(t);
            }
        } else {
            let input = self.task(t).input;
            let input = self.append(packet, input);
            self.task(t).input = input;
        }
        old_task
    }

    fn run_task(&mut self, t: usize) -> TaskRef {
        let mut message = None;
        if self.task(t).is_waiting_with_packet() {
            message = self.task(t).input;
            let link = self.packets[message.expect("message")].link;
            let task = self.task(t);
            task.input = link;
            task.state = if link.is_none() {
                TaskState::create_running() // running()
            } else {
                TaskState {
                    packet_pending: true,
                    task_waiting: false,
                    task_holding: false,
                } // packetPending()
            };
        }
        let handle = self.task(t).handle;
        match handle {
            Data::Device { pending } => self.device_fn(message, pending),
            Data::Handler { work_in, device_in } => self.handler_fn(message, work_in, device_in),
            Data::Idle { control, count } => self.idler_fn(control, count),
            Data::Worker { destination, count } => self.worker_fn(message, destination, count),
        }
    }

    // ---- the task functions: AWFY's closures in createDevice, createHandler, createIdler and
    // createWorker. Each takes a copy of its data record and stores the record back on change.

    fn set_handle(&mut self, handle: Data) {
        self.current().handle = handle;
    }

    fn device_fn(&mut self, work_arg: PacketRef, pending: PacketRef) -> TaskRef {
        let Some(function_work) = work_arg else {
            let Some(function_work) = pending else {
                return self.mark_waiting();
            };
            self.set_handle(Data::Device { pending: None });
            return self.queue_packet(function_work);
        };
        self.set_handle(Data::Device {
            pending: Some(function_work),
        });
        self.hold_self()
    }

    fn handler_fn(&mut self, work: PacketRef, work_in: PacketRef, device_in: PacketRef) -> TaskRef {
        let (mut work_in, mut device_in) = (work_in, device_in);
        if let Some(w) = work {
            if self.packets[w].kind == WORK_PACKET_KIND {
                work_in = self.append(w, work_in);
            } else {
                device_in = self.append(w, device_in);
            }
            self.set_handle(Data::Handler { work_in, device_in });
        }

        let Some(work_packet) = work_in else {
            return self.mark_waiting();
        };
        let count = self.packets[work_packet].datum;
        if count >= DATA_SIZE {
            self.set_handle(Data::Handler {
                work_in: self.packets[work_packet].link,
                device_in,
            });
            return self.queue_packet(work_packet);
        }
        let Some(device_packet) = device_in else {
            return self.mark_waiting();
        };
        self.set_handle(Data::Handler {
            work_in,
            device_in: self.packets[device_packet].link,
        });
        self.packets[device_packet].datum = self.packets[work_packet].data[count];
        self.packets[work_packet].datum = count + 1;
        self.queue_packet(device_packet)
    }

    fn idler_fn(&mut self, control: i32, count: i32) -> TaskRef {
        let count = count - 1;
        if count == 0 {
            self.set_handle(Data::Idle { control, count });
            return self.hold_self();
        }
        if control & 1 == 0 {
            self.set_handle(Data::Idle {
                control: control / 2,
                count,
            });
            return self.release(DEVICE_A);
        }
        self.set_handle(Data::Idle {
            control: (control / 2) ^ 53256,
            count,
        });
        self.release(DEVICE_B)
    }

    fn worker_fn(&mut self, work: PacketRef, destination: usize, count: usize) -> TaskRef {
        let Some(work) = work else {
            return self.mark_waiting();
        };

        let destination = if destination == HANDLER_A {
            HANDLER_B
        } else {
            HANDLER_A
        };
        let mut count = count;
        let packet = &mut self.packets[work];
        packet.identity = destination;
        packet.datum = 0;
        for i in 0..DATA_SIZE {
            count += 1;
            if count > 26 {
                count = 1;
            }
            packet.data[i] = 65 + count - 1;
        }
        self.set_handle(Data::Worker { destination, count });
        self.queue_packet(work)
    }

    // ---- Scheduler

    fn create_packet(&mut self, link: PacketRef, identity: usize, kind: i32) -> PacketRef {
        self.packets.push(Packet {
            link,
            identity,
            kind,
            datum: 0,
            data: [0; DATA_SIZE],
        });
        Some(self.packets.len() - 1)
    }

    fn create_task(
        &mut self,
        identity: usize,
        priority: i32,
        work: PacketRef,
        state: TaskState,
        handle: Data,
    ) {
        let t = TaskControlBlock {
            link: self.task_list,
            identity,
            priority,
            input: work,
            state,
            handle,
        };
        self.task_list = Some(identity);
        self.task_table[identity] = Some(t);
    }

    fn find_task(&mut self, identity: usize) -> usize {
        if self.task_table[identity].is_none() {
            panic!("findTask failed");
        }
        identity
    }

    fn hold_self(&mut self) -> TaskRef {
        self.hold_count += 1;
        let current = self.current();
        current.state.task_holding = true;
        current.link
    }

    fn queue_packet(&mut self, packet: usize) -> TaskRef {
        let t = self.find_task(self.packets[packet].identity);

        self.queue_packet_count += 1;

        self.packets[packet].link = None;
        self.packets[packet].identity = self.current_task_identity;
        self.add_input_and_check_priority(t, packet, self.current_task)
    }

    fn release(&mut self, identity: usize) -> TaskRef {
        let t = self.find_task(identity);
        self.task(t).state.task_holding = false;
        if self.task(t).priority > self.current().priority {
            return Some(t);
        }
        self.current_task
    }

    fn mark_waiting(&mut self) -> TaskRef {
        self.current().state.task_waiting = true;
        self.current_task
    }

    fn schedule(&mut self) {
        self.current_task = self.task_list;
        while let Some(current) = self.current_task {
            if self.task(current).is_task_holding_or_waiting() {
                self.current_task = self.task(current).link;
            } else {
                self.current_task_identity = self.task(current).identity;
                self.current_task = self.run_task(current);
            }
        }
    }

    fn start(&mut self) -> bool {
        self.create_task(
            IDLER,
            0,
            None,
            TaskState::create_running(),
            Data::Idle {
                control: 1,
                count: 10000,
            },
        );
        let mut work_q = self.create_packet(None, WORKER, WORK_PACKET_KIND);
        work_q = self.create_packet(work_q, WORKER, WORK_PACKET_KIND);

        self.create_task(
            WORKER,
            1000,
            work_q,
            TaskState::create_waiting_with_packet(),
            Data::Worker {
                destination: HANDLER_A,
                count: 0,
            },
        );
        work_q = self.create_packet(None, DEVICE_A, DEVICE_PACKET_KIND);
        work_q = self.create_packet(work_q, DEVICE_A, DEVICE_PACKET_KIND);
        work_q = self.create_packet(work_q, DEVICE_A, DEVICE_PACKET_KIND);

        self.create_task(
            HANDLER_A,
            2000,
            work_q,
            TaskState::create_waiting_with_packet(),
            Data::Handler {
                work_in: None,
                device_in: None,
            },
        );
        work_q = self.create_packet(None, DEVICE_B, DEVICE_PACKET_KIND);
        work_q = self.create_packet(work_q, DEVICE_B, DEVICE_PACKET_KIND);
        work_q = self.create_packet(work_q, DEVICE_B, DEVICE_PACKET_KIND);

        self.create_task(
            HANDLER_B,
            3000,
            work_q,
            TaskState::create_waiting_with_packet(),
            Data::Handler {
                work_in: None,
                device_in: None,
            },
        );
        self.create_task(
            DEVICE_A,
            4000,
            None,
            TaskState::create_waiting(),
            Data::Device { pending: None },
        );
        self.create_task(
            DEVICE_B,
            5000,
            None,
            TaskState::create_waiting(),
            Data::Device { pending: None },
        );

        self.schedule();

        self.queue_packet_count == 23246 && self.hold_count == 9297
    }
}

fn run(inner_iterations: i64) -> i64 {
    let mut sum = 0;
    for _ in 0..inner_iterations {
        if Scheduler::new().start() {
            sum += 1;
        }
    }
    sum
}

fn main() {
    sakura_bench_ports::main(run, 100000);
}
