// First run, step 2: once a library first opens after none was chosen, ask
// once where the person writes, which picks the copy format [C12].
import { useEffect, useRef, useState } from "react";
import type { LibraryStatus } from "../../api";

export function useAskWriting(library: LibraryStatus | null | undefined) {
  const [askWriting, setAskWriting] = useState(false);
  const sawNoLibrary = useRef(false);
  useEffect(() => {
    if (library?.state === "none") sawNoLibrary.current = true;
    else if (library?.has_snapshot && library.state === "loaded" && sawNoLibrary.current) {
      sawNoLibrary.current = false;
      setAskWriting(true);
    }
  }, [library?.state, library?.has_snapshot]);
  return [askWriting, setAskWriting] as const;
}
