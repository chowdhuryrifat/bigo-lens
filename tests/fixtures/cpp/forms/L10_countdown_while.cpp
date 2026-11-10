// L10: while (x > 0 | x | x != 0 | x >= 1) with exactly one top-level unit decrement.
int countdown_gt(int x) {
  int steps = 0;
  while (x > 0) {
    x--;
    ++steps;
  }
  return steps;
}

int countdown_truthy(int x) {
  int steps = 0;
  while (x) {
    --x;
    ++steps;
  }
  return steps;
}

int countdown_not_equal(int x) {
  int steps = 0;
  while (x != 0) {
    x -= 1;
    ++steps;
  }
  return steps;
}

int countdown_ge_one(int x) {
  int steps = 0;
  while (x >= 1) {
    ++steps;
    x--;
  }
  return steps;
}

// Near miss: two updates per iteration.
int near_miss_two_updates(int x) {
  int steps = 0;
  while (x > 0) {
    x--;
    x--;
    ++steps;
  }
  return steps;
}

// Near miss: the update is conditional.
int near_miss_conditional_update(int x, int y) {
  int steps = 0;
  while (x > 0) {
    if (y > steps) {
      x--;
    }
    ++steps;
  }
  return steps;
}
