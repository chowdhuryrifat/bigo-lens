// L3: i += c with constant c > 0, condition < or <=.
long long stride_lt(int n) {
  long long s = 0;
  for (int i = 0; i < n; i += 3) {
    s += i;
  }
  return s;
}

long long stride_le(int n) {
  long long s = 0;
  for (int i = 0; i <= n; i += 4) {
    s += i;
  }
  return s;
}

// Near miss: the step is a variable.
long long near_miss_variable_step(int n, int k) {
  long long s = 0;
  for (int i = 0; i < n; i += k) {
    s += i;
  }
  return s;
}

// Near miss: a zero step never terminates.
long long near_miss_zero_step(int n) {
  long long s = 0;
  for (int i = 0; i < n; i += 0) {
    s += i;
  }
  return s;
}
