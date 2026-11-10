// L1: for (T i = a; i < b; ++i), also i++, assignment init and i != b with unit step.
long long lt_preincrement(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    s += i;
  }
  return s;
}

long long lt_postincrement(int n) {
  long long s = 0;
  for (int i = 0; i < n; i++) {
    s += i;
  }
  return s;
}

long long assignment_init(int n) {
  long long s = 0;
  int i = 0;
  for (i = 0; i < n; ++i) {
    s += i;
  }
  return s;
}

long long not_equal_unit_step(int n) {
  long long s = 0;
  for (int i = 0; i != n; ++i) {
    s += i;
  }
  return s;
}

long long nonzero_start(int n) {
  long long s = 0;
  for (int i = 2; i < n; ++i) {
    s += i;
  }
  return s;
}

long long reversed_comparison(int n) {
  long long s = 0;
  for (int i = 0; n > i; ++i) {
    s += i;
  }
  return s;
}

// Near miss: != with a non-unit step can skip the bound.
long long near_miss_not_equal_stride(int n) {
  long long s = 0;
  for (int i = 0; i != n; i += 2) {
    s += i;
  }
  return s;
}

// Near miss: the index is written in the body.
long long near_miss_index_written(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    if (i % 3 == 0) {
      i += 2;
    }
    s += i;
  }
  return s;
}

// Near miss: the bound variable is written in the body (count assumes its initial value).
long long near_miss_bound_written(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    if (i == 10) {
      --n;
    }
    s += i;
  }
  return s;
}

// Near miss: floating-point index.
double near_miss_floating_index(double n) {
  double s = 0;
  for (double x = 0; x < n; x += 1.0) {
    s += x;
  }
  return s;
}

// Near miss: the bound is read from memory, not a supported integer expression.
long long near_miss_bound_from_array(const int* a, int n) {
  long long s = 0;
  for (int i = 0; i < a[n]; ++i) {
    s += i;
  }
  return s;
}
