// Mock OpenAI-compatible endpoint for e2e tests. The app's "custom" LLM
// provider is pointed at http://127.0.0.1:18999/v1/ and this server answers
// model listing and chat completions with a fixed recipe tool call.
import http from 'node:http';

const PORT = 18999;

const TOOL_CALL = JSON.stringify({
	name: 'Mock Pasta',
	ingredients: [
		{ name: 'flour', quantity: '200 g' },
		{ name: 'eggs', quantity: '3' },
	],
	instructions: '<p>Mix the ingredients and cook until done.</p>',
	portion: 4,
});

http
	.createServer((req, res) => {
		if (req.method === 'GET' && req.url === '/v1/models') {
			res.writeHead(200, { 'content-type': 'application/json' });
			res.end(JSON.stringify({ object: 'list', data: [{ id: 'mock-model' }] }));
			return;
		}
		if (req.method === 'POST' && req.url === '/v1/chat/completions') {
			let body = '';
			req.on('data', (chunk) => (body += chunk));
			req.on('end', () => {
				const payload = JSON.parse(body);
				res.writeHead(200, { 'content-type': 'application/json' });
				res.end(
					JSON.stringify({
						id: 'chatcmpl-mock',
						object: 'chat.completion',
						created: 0,
						model: payload.model,
						choices: [
							{
								index: 0,
								message: {
									role: 'assistant',
									content: null,
									tool_calls: [
										{
											id: 'call_mock',
											type: 'function',
											function: { name: 'extract_recipe', arguments: TOOL_CALL },
										},
									],
								},
								finish_reason: 'tool_calls',
							},
						],
						usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
					}),
				);
			});
			return;
		}
		res.writeHead(404);
		res.end();
	})
	.listen(PORT, '127.0.0.1');
