// L6: i *= c or i <<= k from a constant start >= 1.
int doubling(int n) {
  int steps = 0;
  for (int i = 1; i < n; i *= 2) {
    ++steps;
  }
  return steps;
}

int tripling_inclusive(int n) {
  int steps = 0;
  for (int i = 1; i <= n; i *= 3) {
    ++steps;
  }
  return steps;
}

int shift_left(int n) {
  int steps = 0;
  for (int i = 1; i < n; i <<= 2) {
    ++steps;
  }
  return steps;
}

// Near miss: starting at 0, multiplication never grows the index.
int near_miss_zero_start(int n) {
  int steps = 0;
  for (int i = 0; i < n; i *= 2) {
    ++steps;
  }
  return steps;
}

// Near miss: a non-constant start.
int near_miss_variable_start(int n, int a) {
  int steps = 0;
  for (int i = a; i < n; i *= 2) {
    ++steps;
  }
  return steps;
}
