import type { ReactNode } from "react";
import { Code } from "@/components/code";
import { sample } from "./samples";
import { Section, TextLink } from "./section";
import { Tabs } from "./tabs";

// samples/account.sak, checked and tested by scripts/check-doc-sweep.py. The two outputs are
// recorded from `sakura test account.sak` and `sakura verify account.sak` (Z3 installed) on main;
// re-record them when the sample or the output format changes.
const SOURCE = sample("account");

const TEST = `$ sakura test account.sak
test withdraw never overdraws ... ok

1 passed, 0 failed, 0 skipped (seed 0)`;

const VERIFY = `$ sakura verify account.sak
proved      main::account::withdraw overflow t%21 = acct.balance - amount at account.sak:14:57
proved      main::account::withdraw where Cents at account.sak:14:46
proved      main::account::withdraw ensures withdraw at account.sak:11:11
3 proved, 0 disproved, 0 unknown, 0 unsupported (0 reused) (profile js, z3 5.3.1, rlimit 10000000)`;

const C = ({ children }: { children: ReactNode }) => (
  <code className="font-mono text-[0.92em] text-foreground">{children}</code>
);

// The signature on lines 9–11, read clause by clause.
const CLAUSES: { label: string; body: ReactNode }[] = [
  {
    label: "Takes",
    body: (
      <>
        An <C>Account</C> and <C>Cents</C>: an <C>Int</C> that is never negative.
      </>
    ),
  },
  {
    label: "May do",
    body: (
      <>
        <C>Ledger</C>, and fail with <C>Overdrawn</C>. No network, files or clock.
      </>
    ),
  },
  {
    label: "Requires",
    body: (
      <>
        <C>amount &gt; 0</C>. Breaking it is the caller&apos;s bug, reported as <C>D020</C>.
      </>
    ),
  },
  {
    label: "Ensures",
    body: (
      <>
        The new balance, checked on every return; <C>sakura verify</C> proves it ahead of time.
      </>
    ),
  },
  {
    label: "Tests",
    body: (
      <>
        Live beside the code and bring their own fake <C>Ledger</C>.
      </>
    ),
  },
];

export async function CodeSample() {
  return (
    <Section
      id="code"
      ask="What does the code look like?"
      title="Read the signature. The compiler reads the body."
      lede={
        <>
          One file that compiles today. You review the three highlighted lines; <C>check</C>,{" "}
          <C>test</C> and <C>verify</C> make sure the body keeps to them.{" "}
          <TextLink href="/docs/tour/basics">Language tour</TextLink>
        </>
      }
    >
      <div className="grid gap-x-14 gap-y-10 lg:grid-cols-[minmax(0,19rem)_minmax(0,1fr)]">
        <ol className="grid content-start border-t border-foreground/80">
          {CLAUSES.map((c, i) => (
            <li
              key={c.label}
              className="grid grid-cols-[1.5rem_minmax(0,1fr)] gap-x-3 border-b border-border py-3.5"
            >
              <span className="font-mono text-[11.5px] text-faint tabular-nums">{`0${i + 1}`}</span>
              <p className="text-[14.5px] leading-snug text-muted-foreground">
                <span className="font-semibold text-foreground">{c.label}.</span> {c.body}
              </p>
            </li>
          ))}
        </ol>
        <Tabs
          label="The sample, its test run and its proofs"
          items={[
            {
              label: "account.sak",
              panel: <Code code={SOURCE} numbers mark={{ 8: "seal", 9: "seal", 10: "seal" }} />,
            },
            {
              label: "sakura test",
              short: "test",
              panel: <Code code={TEST} lang="text" title={<span>terminal</span>} wrap />,
            },
            {
              label: "sakura verify",
              short: "verify",
              panel: <Code code={VERIFY} lang="text" title={<span>terminal</span>} wrap />,
            },
          ]}
        />
      </div>
    </Section>
  );
}
