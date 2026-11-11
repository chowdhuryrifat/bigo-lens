// L14: two-pointer while (l < r) loops where every path performs ++l or --r.
int two_pointer_basic(int n) {
  int l = 0;
  int r = n;
  int steps = 0;
  while (l < r) {
    if (l * 2 < r) {
      ++l;
    } else {
      --r;
    }
    ++steps;
  }
  return steps;
}

int two_pointer_symmetric(int n) {
  int l = 0;
  int r = n;
  int steps = 0;
  while (r > l) {
    if (l * 2 < r) {
      l++;
    } else {
      r--;
    }
    ++steps;
  }
  return steps;
}

int two_pointer_early_exit(int n) {
  int l = 0;
  int r = n;
  int steps = 0;
  while (l < r) {
    if (l + r == n) {
      return steps;
    } else if (l + r < n) {
      ++l;
    } else {
      --r;
    }
    ++steps;
  }
  return steps;
}

int two_pointer_offset(int n) {
  int l = 0;
  int r = n - 1;
  int steps = 0;
  while (l < r) {
    if (l * 2 < r) {
      ++l;
    } else {
      --r;
    }
    ++steps;
  }
  return steps;
}

int two_pointer_params(int l, int r) {
  int steps = 0;
  while (l < r) {
    if (l * 2 < r) {
      ++l;
    } else {
      --r;
    }
    ++steps;
  }
  return steps;
}

// Near miss: one path updates neither variable.
int near_miss_one_path_no_update(int n) {
  int l = 0;
  int r = n;
  int steps = 0;
  while (l < r) {
    if (l * 2 < r) {
      ++l;
    }
    ++steps;
  }
  return steps;
}

// Near miss: both variables updated on the same path.
int near_miss_two_updates(int n) {
  int l = 0;
  int r = n;
  int steps = 0;
  while (l < r) {
    ++l;
    --r;
    ++steps;
  }
  return steps;
}

// Near miss: update is not ++l or --r.
int near_miss_arbitrary_assignment(int n) {
  int l = 0;
  int r = n;
  int steps = 0;
  while (l < r) {
    if (l * 2 < r) {
      l = l + 2;
    } else {
      --r;
    }
    ++steps;
  }
  return steps;
}
