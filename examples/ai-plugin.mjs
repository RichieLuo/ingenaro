// A minimal AI engine plugin that runs without a model or network connection.
import { createInterface } from 'node:readline';

const input = createInterface({ input: process.stdin });
const send = (frame) => process.stdout.write(JSON.stringify(frame) + '\n');

for await (const line of input) {
  const frame = JSON.parse(line);
  if (frame.type === 'hello') {
    send({ type: 'hello', protocol: 'ai-design-engine', version: 1 });
  } else if (frame.type === 'start') {
    const names = new Set(frame.tools.map((tool) => tool.name));
    const request = frame.prompt.split('\n\nUser request:\n').at(-1);
    if (request.includes('添加示例标题') && names.has('rpshop_batch_edit')) {
      send({
        type: 'tool_call',
        id: 'example-title',
        name: 'rpshop_batch_edit',
        arguments: {
          operations: [{
            op: 'create_widget',
            kind: 'Text',
            properties: { text: '来自自定义引擎的标题', x: 80, y: 80 },
          }],
        },
      });
    } else {
      send({ type: 'text', text: `引擎已连接，宿主提供了 ${names.size} 个工具。发送「添加示例标题」可演示画布编辑。` });
      send({ type: 'done' });
      break;
    }
  } else if (frame.type === 'tool_result' && frame.id === 'example-title') {
    send({
      type: 'text',
      text: frame.is_error ? '本次标题编辑未应用。' : '已添加可编辑的标题。',
    });
    send({ type: 'done' });
    break;
  }
}
