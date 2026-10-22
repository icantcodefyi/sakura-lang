/* Towers from Are We Fast Yet (smarr/are-we-fast-yet@74306fec benchmarks/JavaScript/towers.js;
   Copyright (c) 2015-2016 Stefan Marr, MIT, derived from the SOM benchmarks). Follows AWFY's C++
   port: heap disks linked through a mutable `next`, three piles as a fixed array of tops, disks
   relinked in place by every move and freed through the piles after each benchmark(). An
   illegal move exits 1 with AWFY's message, as the C++ port does. run(n) sums n benchmark()
   results. */
#include <stdio.h>
#include <stdlib.h>

#include "runner.h"

typedef struct TowersDisk {
  int64_t size;
  struct TowersDisk *next;
} TowersDisk;

typedef struct {
  TowersDisk *piles[3];
  int64_t moves_done;
} Towers;

static void die(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(1);
}

static void push_disk(Towers *self, TowersDisk *disk, int64_t pile) {
  TowersDisk *top = self->piles[pile];
  if (top != NULL && disk->size >= top->size) die("Cannot put a big disk on a smaller one");

  disk->next = top;
  self->piles[pile] = disk;
}

static TowersDisk *pop_disk_from(Towers *self, int64_t pile) {
  TowersDisk *top = self->piles[pile];
  if (top == NULL) die("Attempting to remove a disk from an empty pile");

  self->piles[pile] = top->next;
  top->next = NULL;
  return top;
}

static void move_top_disk(Towers *self, int64_t from_pile, int64_t to_pile) {
  push_disk(self, pop_disk_from(self, from_pile), to_pile);
  self->moves_done += 1;
}

static void build_tower_at(Towers *self, int64_t pile, int64_t disks) {
  for (int64_t i = disks; i >= 0; i -= 1) {
    TowersDisk *disk = malloc(sizeof *disk);
    if (!disk) die("out of memory");
    disk->size = i;
    disk->next = NULL;
    push_disk(self, disk, pile);
  }
}

static void move_disks(Towers *self, int64_t disks, int64_t from_pile, int64_t to_pile) {
  if (disks == 1) {
    move_top_disk(self, from_pile, to_pile);
  } else {
    const int64_t other_pile = (3 - from_pile) - to_pile;
    move_disks(self, disks - 1, from_pile, other_pile);
    move_top_disk(self, from_pile, to_pile);
    move_disks(self, disks - 1, other_pile, to_pile);
  }
}

static int64_t benchmark(Towers *self) {
  for (int i = 0; i < 3; i += 1) self->piles[i] = NULL;
  build_tower_at(self, 0, 13);
  self->moves_done = 0;
  move_disks(self, 13, 0, 1);

  for (int i = 0; i < 3; i += 1) {
    TowersDisk *disk = self->piles[i];
    while (disk != NULL) {
      TowersDisk *next = disk->next;
      free(disk);
      disk = next;
    }
  }
  return self->moves_done;
}

static int64_t run(int64_t n) {
  Towers bench = {{NULL, NULL, NULL}, 0};
  int64_t sum = 0;
  for (int64_t i = 0; i < n; i += 1) sum += benchmark(&bench);
  return sum;
}

int main(int argc, char **argv) { return bench_main(argc, argv, run, 2500); }
