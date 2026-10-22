// Richards from Are We Fast Yet (benchmarks/JavaScript/richards.js), "a derivation from the SOM
// version, which is derived from Mario Wolczko's Smalltalk version"; see ../LICENSE.
// Deviations are the Sakura port's: task control blocks (by identity) and packets (by creation
// order) are records in vectors threaded through every scheduler call, as the DeltaBlue Koka port
// does; Perceus updates the uniquely owned vectors in place. Every reference is an index and -1
// is NO_TASK / NO_WORK. AWFY's task functions are closures made with their data record; here the
// data record (`data`, as `handle` is a Koka keyword) is a variant and run-task matches on it, as
// every port of this program does. The idler's control word (`ctrl`: `control` is a Koka keyword
// too) is an int64 for `and` and `xor`. TRACING is false in AWFY; its trace code is left out.
// run(n) runs benchmark() n times and returns the number of true results.
import std/num/int64
import runner

val no-task = -1
val no-work = -1
val idler = 0
val worker = 1
val handler-a = 2
val handler-b = 3
val device-a = 4
val device-b = 5
val num-types = 6

val device-packet-kind = 0
val work-packet-kind = 1

val data-size = 4

struct packet
  link: int
  identity: int
  kind: int
  datum: int
  data: vector<int>

// The task data records; the variant selects the task function.
type data
  DeviceData(pending: int)
  HandlerData(work-in: int, device-in: int)
  IdleData(ctrl: int64, count: int)
  WorkerData(destination: int, wcount: int)

// AWFY's TaskState flags, the initial state a TaskControlBlock copies.
struct task-state
  pending: bool
  waiting: bool
  holding: bool

struct tcb
  link: int
  identity: int
  priority: int
  input: int
  packet-pending: bool
  task-waiting: bool
  task-holding: bool
  rec: data

struct sched
  tasks: vector<tcb>
  packets: vector<packet>
  queue-packet-count: int
  hold-count: int
  task-list: int
  current-task: int
  current-task-identity: int

fun task(s: sched, id: int): exn tcb
  s.tasks[id]

fun packet(s: sched, id: int): exn packet
  s.packets[id]

// Destructure first so the vectors stay uniquely owned and update in place.
fun put-task(s: sched, id: int, t: tcb): exn sched
  match s
    Sched(tasks, packets, qpc, hc, tl, ct, cti) ->
      var ts := tasks
      ts[id] := t
      Sched(ts, packets, qpc, hc, tl, ct, cti)

fun put-packet(s: sched, id: int, p: packet): exn sched
  match s
    Sched(tasks, packets, qpc, hc, tl, ct, cti) ->
      var ps := packets
      ps[id] := p
      Sched(tasks, ps, qpc, hc, tl, ct, cti)

// The current task's data record is replaced.
fun put-handle(s: sched, d: data): exn sched
  val t = task(s, s.current-task)
  put-task(s, s.current-task, t(rec = d))

// RBObject's append: the queue headed by queue-head with p at its end; its new head.
fun append(s0: sched, p: int, queue-head: int): <div,exn> (sched, int)
  val s1 = put-packet(s0, p, packet(s0, p)(link = no-work))
  if no-work == queue-head then return (s1, p)
  var mouse := queue-head
  var next := packet(s1, mouse).link
  while { no-work != next }
    mouse := next
    next := packet(s1, mouse).link
  val m = mouse
  (put-packet(s1, m, packet(s1, m)(link = p)), queue-head)

// ---- TaskState

fun is-task-holding-or-waiting(t: tcb): bool
  t.task-holding || (!t.packet-pending && t.task-waiting)

fun is-waiting-with-packet(t: tcb): bool
  t.packet-pending && t.task-waiting && !t.task-holding

fun create-running(): task-state
  Task-state(False, False, False)

fun create-waiting(): task-state
  Task-state(False, True, False)

fun create-waiting-with-packet(): task-state
  Task-state(True, True, False)

// ---- TaskControlBlock

fun add-input-and-check-priority(s: sched, id: int, p: int, old-task: int): <div,exn> (sched, int)
  val t = task(s, id)
  if no-work == t.input then
    val s1 = put-task(s, id, t(input = p, packet-pending = True))
    if t.priority > task(s1, old-task).priority then (s1, id) else (s1, old-task)
  else
    val (s1, input) = append(s, p, t.input)
    val t1 = task(s1, id)
    (put-task(s1, id, t1(input = input)), old-task)

fun run-task(s0: sched, id: int): <div,exn> (sched, int)
  val t = task(s0, id)
  val (s, message) =
    if is-waiting-with-packet(t) then
      val message = t.input
      val input = packet(s0, message).link
      if no-work == input then // running()
        (put-task(s0, id, t(input = input, packet-pending = False, task-waiting = False, task-holding = False)), message)
      else // packetPending()
        (put-task(s0, id, t(input = input, packet-pending = True, task-waiting = False, task-holding = False)), message)
    else (s0, no-work)
  match task(s, id).rec
    DeviceData(pending) -> device-fn(s, message, pending)
    HandlerData(work-in, device-in) -> handler-fn(s, message, work-in, device-in)
    IdleData(ctrl, count) -> idler-fn(s, ctrl, count)
    WorkerData(destination, count) -> worker-fn(s, message, destination, count)

// ---- the task functions (AWFY's closures in createDevice, createHandler, createIdler and
// createWorker); each returns the scheduler and the next task

fun device-fn(s: sched, work: int, pending: int): <div,exn> (sched, int)
  if no-work == work then
    if no-work == pending then return mark-waiting(s)
    return queue-packet(put-handle(s, DeviceData(no-work)), pending)
  hold-self(put-handle(s, DeviceData(work)))

fun handler-fn(s0: sched, work: int, work-in0: int, device-in0: int): <div,exn> (sched, int)
  val (s, work-in, device-in) =
    if no-work != work then
      if work-packet-kind == packet(s0, work).kind then
        val (s1, head) = append(s0, work, work-in0)
        (put-handle(s1, HandlerData(head, device-in0)), head, device-in0)
      else
        val (s1, head) = append(s0, work, device-in0)
        (put-handle(s1, HandlerData(work-in0, head)), work-in0, head)
    else (s0, work-in0, device-in0)
  val work-packet = work-in
  if no-work == work-packet then return mark-waiting(s)
  val wp = packet(s, work-packet)
  val count = wp.datum
  if count >= data-size then
    return queue-packet(put-handle(s, HandlerData(wp.link, device-in)), work-packet)
  val device-packet = device-in
  if no-work == device-packet then return mark-waiting(s)
  val s1 = put-handle(s, HandlerData(work-in, packet(s, device-packet).link))
  val dp = packet(s1, device-packet)
  val s2 = put-packet(s1, device-packet, dp(datum = wp.data[count]))
  val wp2 = packet(s2, work-packet)
  val s3 = put-packet(s2, work-packet, wp2(datum = count + 1))
  queue-packet(s3, device-packet)

fun idler-fn(s: sched, ctrl: int64, count0: int): <div,exn> (sched, int)
  val count = count0 - 1
  if 0 == count then return hold-self(put-handle(s, IdleData(ctrl, count)))
  if ctrl.and(1.int64) == 0.int64 then
    return release(put-handle(s, IdleData(ctrl / 2.int64, count)), device-a)
  release(put-handle(s, IdleData((ctrl / 2.int64).xor(53256.int64), count)), device-b)

fun worker-fn(s: sched, work: int, destination0: int, count0: int): <div,exn> (sched, int)
  if no-work == work then return mark-waiting(s)
  val destination = if handler-a == destination0 then handler-b else handler-a
  val p = packet(s, work)
  var d := p.data
  var count := count0
  for(0, data-size - 1) fn(i)
    count := count + 1
    if count > 26 then count := 1
    d[i] := 65 + count - 1
  val s1 = put-handle(s, WorkerData(destination, count))
  queue-packet(put-packet(s1, work, p(identity = destination, datum = 0, data = d)), work)

// ---- Scheduler

fun create-packet(s: sched, link: int, identity: int, kind: int): (sched, int)
  match s
    Sched(tasks, packets, qpc, hc, tl, ct, cti) ->
      val p = Packet(link, identity, kind, 0, vector(data-size, 0))
      val n = packets.length
      (Sched(tasks, (packets.vlist ++ [p]).vector, qpc, hc, tl, ct, cti), n)

fun create-task(s: sched, identity: int, priority: int, work: int, state: task-state, d: data): exn sched
  val t = Tcb(s.task-list, identity, priority, work, state.pending, state.waiting, state.holding, d)
  put-task(s, identity, t)(task-list = identity)

fun find-task(s: sched, id: int): exn int
  if no-task == task(s, id).identity then throw("findTask failed")
  id

fun hold-self(s: sched): exn (sched, int)
  val t = task(s, s.current-task)
  val s1 = put-task(s, s.current-task, t(task-holding = True))
  (s1(hold-count = s1.hold-count + 1), t.link)

fun queue-packet(s: sched, p: int): <div,exn> (sched, int)
  val t = find-task(s, packet(s, p).identity)
  if no-task == t then return (s, no-task)
  val s1 = s(queue-packet-count = s.queue-packet-count + 1)
  val s2 = put-packet(s1, p, packet(s1, p)(link = no-work, identity = s1.current-task-identity))
  add-input-and-check-priority(s2, t, p, s2.current-task)

fun release(s: sched, identity: int): exn (sched, int)
  val id = find-task(s, identity)
  if no-task == id then return (s, no-task)
  val s1 = put-task(s, id, task(s, id)(task-holding = False))
  if task(s1, id).priority > task(s1, s1.current-task).priority then (s1, id)
  else (s1, s1.current-task)

fun mark-waiting(s: sched): exn (sched, int)
  (put-task(s, s.current-task, task(s, s.current-task)(task-waiting = True)), s.current-task)

fun schedule(start: sched): <div,exn> sched
  var s := start(current-task = start.task-list)
  while { no-task != s.current-task }
    val t = task(s, s.current-task)
    if is-task-holding-or-waiting(t) then
      s := s(current-task = t.link)
    else
      val s0 = s(current-task-identity = t.identity)
      val (s1, next) = run-task(s0, s0.current-task)
      s := s1(current-task = next)
  s

fun new-scheduler(): sched
  val none = Tcb(no-task, no-task, 0, no-work, False, False, False, DeviceData(no-work))
  Sched(vector(num-types, none), vector(0, Packet(no-work, 0, 0, 0, vector(0, 0))), 0, 0, no-task, no-task, 0)

// AWFY's Scheduler.start (`start` would clash with Koka's C runtime symbol kk_main_start).
fun start-scheduler(): <div,exn> bool
  val s0 = create-task(new-scheduler(), idler, 0, no-work, create-running(), IdleData(1.int64, 10000))
  val (s1, q1) = create-packet(s0, no-work, worker, work-packet-kind)
  val (s2, q2) = create-packet(s1, q1, worker, work-packet-kind)

  val s3 = create-task(s2, worker, 1000, q2, create-waiting-with-packet(), WorkerData(handler-a, 0))
  val (s4, q4) = create-packet(s3, no-work, device-a, device-packet-kind)
  val (s5, q5) = create-packet(s4, q4, device-a, device-packet-kind)
  val (s6, q6) = create-packet(s5, q5, device-a, device-packet-kind)

  val s7 = create-task(s6, handler-a, 2000, q6, create-waiting-with-packet(), HandlerData(no-work, no-work))
  val (s8, q8) = create-packet(s7, no-work, device-b, device-packet-kind)
  val (s9, q9) = create-packet(s8, q8, device-b, device-packet-kind)
  val (s10, q10) = create-packet(s9, q9, device-b, device-packet-kind)

  val s11 = create-task(s10, handler-b, 3000, q10, create-waiting-with-packet(), HandlerData(no-work, no-work))
  val s12 = create-task(s11, device-a, 4000, no-work, create-waiting(), DeviceData(no-work))
  val s13 = create-task(s12, device-b, 5000, no-work, create-waiting(), DeviceData(no-work))

  val s = schedule(s13)

  s.queue-packet-count == 23246 && s.hold-count == 9297

fun run(inner-iterations: int): <div,exn> int
  var sum := 0
  for(inner-iterations) fn(_)
    if start-scheduler() then sum := sum + 1
  sum

fun main() { bench-main(run, 100000) }
