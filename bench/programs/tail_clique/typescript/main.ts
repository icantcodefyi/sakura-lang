export function run(n: number) {
  let state: "a" | "b" | "c" = "a";
  let acc = 0;
  while (n !== 0) {
    let weight = 0;
    switch (state) {
      case "a":
        if (n % 2 === 0) { state = "b"; weight = 1; }
        else { state = "c"; weight = 2; }
        break;
      case "b":
        if (n % 3 === 0) { state = "a"; weight = 3; }
        else { state = "c"; weight = 4; }
        break;
      case "c":
        if (n % 5 === 0) { state = "a"; weight = 5; }
        else { state = "b"; weight = 6; }
        break;
    }
    n = n - 1;
    if (!Number.isSafeInteger(n)) throw new RangeError("Int overflow");
    acc = acc + weight;
    if (!Number.isSafeInteger(acc)) throw new RangeError("Int overflow");
  }
  return acc;
}

import { main } from "../../_shared/typescript/runner.ts";
if (import.meta.main) await main(run, 10000000);
