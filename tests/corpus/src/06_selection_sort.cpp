// 06_selection_sort.cpp
// Pattern: Triangular loop
// Expected: O(n^2), Proven

int elements[5005];

void selection_sort(int n) {
  for (int i = 0; i < n; ++i) {
    int min_idx = i;
    for (int j = i + 1; j < n; ++j) {
      if (elements[j] < elements[min_idx]) {
        min_idx = j;
      }
    }
    int temp = elements[i];
    elements[i] = elements[min_idx];
    elements[min_idx] = temp;
  }
}
