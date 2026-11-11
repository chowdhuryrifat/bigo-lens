// Status and confidence rules (COMPLEXITY_MODEL.md §3).
int helper_cycle(int x);
int helper(int x) {
  return helper_cycle(x);
}
int helper_cycle(int x) {
  return helper(x);
}
int external_function(int x);

// Calls a user-defined function with a cycle: Estimated, low confidence.
long long calls_user_function(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    s += helper(i);
  }
  return s;
}

// Calls a function without a definition in this file: library call, still Proven.
long long calls_library_function(int n) {
  long long s = 0;
  for (int i = 0; i < n; ++i) {
    s += external_function(i);
  }
  return s;
}

// Early exit: still an upper bound, medium confidence.
int early_exit(int n, const int* a) {
  for (int i = 0; i < n; ++i) {
    if (a[i] == 0) {
      return i;
    }
  }
  return -1;
}

// Unsupported: do-while.
int do_while(int n) {
  int i = 0;
  do {
    ++i;
  } while (i < n);
  return i;
}

// Unknown: infinite for with an internal break.
int endless_for(const int* a) {
  int i = 0;
  for (;;) {
    if (a[i] == 0) {
      break;
    }
    ++i;
  }
  return i;
}

// Recursion with a single self-call.
long long factorial(int n) {
  return n <= 1 ? 1 : n * factorial(n - 1);
}
