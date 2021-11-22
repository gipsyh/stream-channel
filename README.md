# stream-channel

byte stream between threads

## example
```rust
let (mut l, mut r) = StreamChannel::new();
let send = vec![1, 2, 3, 4];
l.write(&send).unwrap();
l.flush().unwrap();
let mut recv = vec![0; 2];
r.read_exact(recv.as_mut()).unwrap();
assert_eq!(recv, vec![1, 2]);
r.read_exact(recv.as_mut()).unwrap();
assert_eq!(recv, vec![3, 4]);
```