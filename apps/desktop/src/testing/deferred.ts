/** A promise a test settles by hand, to hold a reply back and release it later. */
export function deferred<T = void>() {
  let resolve!: (value: T) => void, reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
