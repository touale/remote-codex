import { useEffect, useState } from 'react';
import { MarkdownBody } from '../src/chat/MarkdownBody';

const formula = String.raw`\max_I \frac{1}{K}\sum_k L_{R_k}(y\mid I,s)`;
const sample = String.raw`Find the optimum with \(x_i^2 + y_j^2\).

[ ${formula} ]

\[
\begin{aligned}
f(x) &= \frac{x^2}{1+x} \\
A &= \begin{pmatrix}1 & 2 \\ 3 & 4\end{pmatrix}
\end{aligned}
\]`;
const wide = `\\[${Array.from({ length: 20 }, (_, i) => `a_{${i}}x^{${i}}`).join(' + ')}\\]`;
const streamed = `The streamed result is \\[${formula}\\]`;
const report = (error: unknown) => {
  throw error;
};

export function MathFixture() {
  const [stream, setStream] = useState<string | null>(null);
  useEffect(() => {
    if (stream === null || stream.length === streamed.length) return;
    const timeout = setTimeout(() => setStream(streamed.slice(0, stream.length + 5)), 50);
    return () => clearTimeout(timeout);
  }, [stream]);
  return (
    <div id="fixture-math" style={{ overflow: 'auto', minHeight: 0 }}>
      <div style={{ width: 420, maxWidth: '100%' }}>
        <div className="message-body" id="fixture-formulas">
          <MarkdownBody text={sample} report={report} />
        </div>
        <div className="message-body" id="fixture-wide-math">
          <MarkdownBody text={wide} report={report} />
        </div>
        <button onClick={() => setStream('The streamed result is \\[')}>Stream formula</button>
        <div className="message-body" id="fixture-stream-math">
          <MarkdownBody text={stream ?? ''} report={report} />
        </div>
        <div className="message-body">
          <MarkdownBody text={'```latex\n' + formula + '\n```'} report={report} />
        </div>
      </div>
    </div>
  );
}
