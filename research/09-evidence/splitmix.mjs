// Independent research probe. Constants/algorithm: Sebastiano Vigna's public-domain
// https://prng.di.unimi.it/splitmix64.c (2015). Not the current Sakura generator.
const expected = ['e220a8397b1dcdaf','6e789e6aa1b965f4','06c45d188009454f','f88bb8a8724c81ec','1b39896a51a8749b'];
const word = x => BigInt.asUintN(64, x);
let state = 0n;
for (const want of expected) {
  state = word(state + 0x9e3779b97f4a7c15n);
  let z = state;
  z = word((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
  z = word((z ^ (z >> 27n)) * 0x94d049bb133111ebn);
  const got = word(z ^ (z >> 31n)).toString(16).padStart(16, '0');
  if (got !== want) throw Error(`${got} != ${want}`);
  console.log(got);
}
