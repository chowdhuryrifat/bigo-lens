// L8: i * i <= b or i * i < b with unit step.
bool is_prime(int n) {
  for (int i = 2; i * i <= n; ++i) {
    if (n % i == 0) {
      return false;
    }
  }
  return n > 1;
}

int count_below(int n) {
  int c = 0;
  for (int i = 0; i * i < n; ++i) {
    ++c;
  }
  return c;
}

// Near miss: non-unit step.
int near_miss_stride(int n) {
  int c = 0;
  for (int i = 1; i * i <= n; i += 2) {
    ++c;
  }
  return c;
}
