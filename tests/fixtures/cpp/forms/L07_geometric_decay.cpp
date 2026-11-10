// L7: i /= c or i >>= k while the index is positive.
int halving(int n) {
  int steps = 0;
  for (int i = n; i > 0; i /= 2) {
    ++steps;
  }
  return steps;
}

int shift_right(int n) {
  int steps = 0;
  for (int i = n; i >= 1; i >>= 1) {
    ++steps;
  }
  return steps;
}

int not_equal_zero(int n) {
  int steps = 0;
  for (int i = n; i != 0; i /= 10) {
    ++steps;
  }
  return steps;
}

// Near miss: stops at a bound other than "positive".
int near_miss_stop_above_one(int n) {
  int steps = 0;
  for (int i = n; i > 1; i /= 2) {
    ++steps;
  }
  return steps;
}
