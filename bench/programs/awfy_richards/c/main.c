/* Richards from Are We Fast Yet (benchmarks/JavaScript/richards.js), "a derivation from the SOM
   version, which is derived from Mario Wolczko's Smalltalk version"; see ../LICENSE.
   AWFY's objects are heap structs linked by pointers (NULL is NO_TASK / NO_WORK), mutated in
   place and freed when benchmark() returns. Deviation: AWFY's task functions are closures made
   with their data record; here the data record is a tagged union and run_task switches on its
   tag, as every port of this program does. TRACING is false in AWFY; its trace code is left out.
   run(n) runs benchmark() n times and returns the number of true results. */
#include "runner.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

enum { IDLER, WORKER, HANDLER_A, HANDLER_B, DEVICE_A, DEVICE_B, NUM_TYPES };
enum { DEVICE_PACKET_KIND, WORK_PACKET_KIND };
enum { DATA_SIZE = 4 };

static void die(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(1);
}

static void *alloc(size_t size) {
  void *p = calloc(1, size);
  if (!p) die("out of memory");
  return p;
}

struct packet {
  struct packet *link;
  int identity;
  int kind;
  int datum;
  int data[DATA_SIZE];
};

/* RBObject's append. */
static struct packet *append(struct packet *packet, struct packet *queue_head) {
  packet->link = NULL;
  if (queue_head == NULL) return packet;

  struct packet *mouse = queue_head;
  struct packet *link;

  while ((link = mouse->link) != NULL) mouse = link;
  mouse->link = packet;
  return queue_head;
}

struct task_state {
  bool packet_pending, task_waiting, task_holding;
};

static struct task_state create_running(void) { return (struct task_state){false, false, false}; }
static struct task_state create_waiting(void) { return (struct task_state){false, true, false}; }
static struct task_state create_waiting_with_packet(void) {
  return (struct task_state){true, true, false};
}

/* The task data records; the tag selects the task function. */
enum data_kind { DEVICE_DATA, HANDLER_DATA, IDLE_DATA, WORKER_DATA };
struct data_record {
  enum data_kind kind;
  union {
    struct { struct packet *pending; } device;
    struct { struct packet *work_in, *device_in; } handler;
    struct { int control, count; } idle;
    struct { int destination, count; } worker;
  };
};

struct tcb {
  struct tcb *link;
  int identity;
  int priority;
  struct packet *input;
  bool packet_pending, task_waiting, task_holding;
  struct data_record *handle;
};

static bool is_task_holding_or_waiting(const struct tcb *t) {
  return t->task_holding || (!t->packet_pending && t->task_waiting);
}

static bool is_waiting_with_packet(const struct tcb *t) {
  return t->packet_pending && t->task_waiting && !t->task_holding;
}

static struct tcb *add_input_and_check_priority(struct tcb *t, struct packet *packet,
                                                struct tcb *old_task) {
  if (t->input == NULL) {
    t->input = packet;
    t->packet_pending = true;
    if (t->priority > old_task->priority) return t;
  } else {
    t->input = append(packet, t->input);
  }
  return old_task;
}

struct scheduler {
  int queue_packet_count;
  int hold_count;
  struct tcb *task_table[NUM_TYPES];
  struct tcb *task_list;
  struct tcb *current_task;
  int current_task_identity;
  struct packet *packets[16]; /* every packet created, to free them */
  int num_packets;
};

static struct tcb *find_task(struct scheduler *s, int identity) {
  struct tcb *t = s->task_table[identity];
  if (t == NULL) die("findTask failed");
  return t;
}

static struct tcb *hold_self(struct scheduler *s) {
  s->hold_count += 1;
  s->current_task->task_holding = true;
  return s->current_task->link;
}

static struct tcb *queue_packet(struct scheduler *s, struct packet *packet) {
  struct tcb *t = find_task(s, packet->identity);
  if (t == NULL) return NULL;

  s->queue_packet_count += 1;

  packet->link = NULL;
  packet->identity = s->current_task_identity;
  return add_input_and_check_priority(t, packet, s->current_task);
}

static struct tcb *release(struct scheduler *s, int identity) {
  struct tcb *t = find_task(s, identity);
  if (t == NULL) return NULL;
  t->task_holding = false;
  if (t->priority > s->current_task->priority) return t;
  return s->current_task;
}

static struct tcb *mark_waiting(struct scheduler *s) {
  s->current_task->task_waiting = true;
  return s->current_task;
}

/* The task functions: AWFY's closures in createDevice, createHandler, createIdler and
   createWorker. */

static struct tcb *device_fn(struct scheduler *s, struct packet *work_arg, struct data_record *d) {
  struct packet *function_work = work_arg;
  if (function_work == NULL) {
    if ((function_work = d->device.pending) == NULL) return mark_waiting(s);
    d->device.pending = NULL;
    return queue_packet(s, function_work);
  }
  d->device.pending = function_work;
  return hold_self(s);
}

static struct tcb *handler_fn(struct scheduler *s, struct packet *work, struct data_record *d) {
  if (work != NULL) {
    if (work->kind == WORK_PACKET_KIND) {
      d->handler.work_in = append(work, d->handler.work_in);
    } else {
      d->handler.device_in = append(work, d->handler.device_in);
    }
  }

  struct packet *work_packet;
  if ((work_packet = d->handler.work_in) == NULL) return mark_waiting(s);
  int count = work_packet->datum;
  if (count >= DATA_SIZE) {
    d->handler.work_in = work_packet->link;
    return queue_packet(s, work_packet);
  }
  struct packet *device_packet;
  if ((device_packet = d->handler.device_in) == NULL) return mark_waiting(s);
  d->handler.device_in = device_packet->link;
  device_packet->datum = work_packet->data[count];
  work_packet->datum = count + 1;
  return queue_packet(s, device_packet);
}

static struct tcb *idler_fn(struct scheduler *s, struct data_record *d) {
  d->idle.count -= 1;
  if (d->idle.count == 0) return hold_self(s);
  if ((d->idle.control & 1) == 0) {
    d->idle.control /= 2;
    return release(s, DEVICE_A);
  }
  d->idle.control = (d->idle.control / 2) ^ 53256;
  return release(s, DEVICE_B);
}

static struct tcb *worker_fn(struct scheduler *s, struct packet *work, struct data_record *d) {
  if (work == NULL) return mark_waiting(s);

  d->worker.destination = d->worker.destination == HANDLER_A ? HANDLER_B : HANDLER_A;
  work->identity = d->worker.destination;
  work->datum = 0;
  for (int i = 0; i < DATA_SIZE; i++) {
    d->worker.count += 1;
    if (d->worker.count > 26) d->worker.count = 1;
    work->data[i] = 65 + d->worker.count - 1;
  }
  return queue_packet(s, work);
}

static struct tcb *run_task(struct scheduler *s, struct tcb *t) {
  struct packet *message;
  if (is_waiting_with_packet(t)) {
    message = t->input;
    t->input = message->link;
    if (t->input == NULL) {
      t->packet_pending = false; /* running() */
      t->task_waiting = false;
      t->task_holding = false;
    } else {
      t->packet_pending = true; /* packetPending() */
      t->task_waiting = false;
      t->task_holding = false;
    }
  } else {
    message = NULL;
  }
  struct data_record *d = t->handle;
  switch (d->kind) {
    case DEVICE_DATA: return device_fn(s, message, d);
    case HANDLER_DATA: return handler_fn(s, message, d);
    case IDLE_DATA: return idler_fn(s, d);
    case WORKER_DATA: return worker_fn(s, message, d);
  }
  die("unknown task data");
  return NULL;
}

static struct packet *create_packet(struct scheduler *s, struct packet *link, int identity,
                                    int kind) {
  struct packet *p = alloc(sizeof *p);
  p->link = link;
  p->identity = identity;
  p->kind = kind;
  p->datum = 0;
  for (int i = 0; i < DATA_SIZE; i++) p->data[i] = 0;
  s->packets[s->num_packets++] = p;
  return p;
}

static void create_task(struct scheduler *s, int identity, int priority, struct packet *work,
                        struct task_state state, struct data_record data) {
  struct tcb *t = alloc(sizeof *t);
  t->link = s->task_list;
  t->identity = identity;
  t->priority = priority;
  t->input = work;
  t->packet_pending = state.packet_pending;
  t->task_waiting = state.task_waiting;
  t->task_holding = state.task_holding;
  t->handle = alloc(sizeof *t->handle);
  *t->handle = data;
  s->task_list = t;
  s->task_table[identity] = t;
}

static void schedule(struct scheduler *s) {
  s->current_task = s->task_list;
  while (s->current_task != NULL) {
    if (is_task_holding_or_waiting(s->current_task)) {
      s->current_task = s->current_task->link;
    } else {
      s->current_task_identity = s->current_task->identity;
      s->current_task = run_task(s, s->current_task);
    }
  }
}

static bool start(void) {
  struct scheduler s = {0};
  struct packet *work_q;

  create_task(&s, IDLER, 0, NULL, create_running(),
              (struct data_record){.kind = IDLE_DATA, .idle = {1, 10000}});
  work_q = create_packet(&s, NULL, WORKER, WORK_PACKET_KIND);
  work_q = create_packet(&s, work_q, WORKER, WORK_PACKET_KIND);

  create_task(&s, WORKER, 1000, work_q, create_waiting_with_packet(),
              (struct data_record){.kind = WORKER_DATA, .worker = {HANDLER_A, 0}});
  work_q = create_packet(&s, NULL, DEVICE_A, DEVICE_PACKET_KIND);
  work_q = create_packet(&s, work_q, DEVICE_A, DEVICE_PACKET_KIND);
  work_q = create_packet(&s, work_q, DEVICE_A, DEVICE_PACKET_KIND);

  create_task(&s, HANDLER_A, 2000, work_q, create_waiting_with_packet(),
              (struct data_record){.kind = HANDLER_DATA, .handler = {NULL, NULL}});
  work_q = create_packet(&s, NULL, DEVICE_B, DEVICE_PACKET_KIND);
  work_q = create_packet(&s, work_q, DEVICE_B, DEVICE_PACKET_KIND);
  work_q = create_packet(&s, work_q, DEVICE_B, DEVICE_PACKET_KIND);

  create_task(&s, HANDLER_B, 3000, work_q, create_waiting_with_packet(),
              (struct data_record){.kind = HANDLER_DATA, .handler = {NULL, NULL}});
  create_task(&s, DEVICE_A, 4000, NULL, create_waiting(),
              (struct data_record){.kind = DEVICE_DATA, .device = {NULL}});
  create_task(&s, DEVICE_B, 5000, NULL, create_waiting(),
              (struct data_record){.kind = DEVICE_DATA, .device = {NULL}});

  schedule(&s);

  bool ok = s.queue_packet_count == 23246 && s.hold_count == 9297;
  for (int i = 0; i < NUM_TYPES; i++) {
    if (s.task_table[i]) {
      free(s.task_table[i]->handle);
      free(s.task_table[i]);
    }
  }
  for (int i = 0; i < s.num_packets; i++) free(s.packets[i]);
  return ok;
}

static int64_t run(int64_t inner_iterations) {
  int64_t sum = 0;
  for (int64_t i = 0; i < inner_iterations; i++) {
    if (start()) sum += 1;
  }
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 100000); }
