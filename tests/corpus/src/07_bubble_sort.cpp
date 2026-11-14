// 07_bubble_sort.cpp
// Pattern: Nested loop
// Expected: O(n^2), Proven

int data[5005];

void bubble_sort(int n) {
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n - 1; ++j) {
      if (data[j] > data[j + 1]) {
        int t = data[j];
        data[j] = data[j + 1];
        data[j + 1] = t;
      }
    }
  }
}
