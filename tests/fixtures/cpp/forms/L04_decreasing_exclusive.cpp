// L4: for (T i = a; i > b; --i), also i != b with unit step.
long long gt_predecrement(int n) {
  long long s = 0;
  for (int i = n; i > 0; --i) {
    s += i;
  }
  return s;
}

long long not_equal_decrement(int n) {
  long long s = 0;
  for (int i = n; i != 0; i--) {
    s += i;
  }
  return s;
}

// Near miss: the decrement is a variable.
long long near_miss_variable_decrement(int n, int k) {
  long long s = 0;
  for (int i = n; i > 0; i -= k) {
    s += i;
  }
  return s;
}

// Near miss: decreasing index with a condition that does not bound it from below.
long long near_miss_wrong_direction(int n) {
  long long s = 0;
  for (int i = n; i < 100; --i) {
    s += i;
  }
  return s;
}
