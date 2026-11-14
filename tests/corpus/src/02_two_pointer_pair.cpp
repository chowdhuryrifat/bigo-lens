// 02_two_pointer_pair.cpp
// Pattern: Two-pointer while loop
// Expected: O(n), Proven

int a[200005];

int two_pointer_target_sum(int n, int target) {
  int l = 0;
  int r = n - 1;
  int count = 0;
  while (l < r) {
    int current = a[l] + a[r];
    if (current == target) {
      count++;
      ++l;
    } else if (current < target) {
      ++l;
    } else {
      --r;
    }
  }
  return count;
}
