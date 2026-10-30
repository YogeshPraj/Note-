// Scratch test — verify _balanceJsonBraces against the user's broken JSON.
// Run with: node tools/test-brace-balancer.js

function _balanceJsonBraces(content) {
  const stack = [];
  let inString = false;
  let escape = false;
  let inLineComment = false, inBlockComment = false;
  for (let i = 0; i < content.length; i++) {
    const ch = content[i];
    const next = content[i + 1];
    if (inLineComment) { if (ch === '\n') inLineComment = false; continue; }
    if (inBlockComment) { if (ch === '*' && next === '/') { inBlockComment = false; i++; } continue; }
    if (escape) { escape = false; continue; }
    if (inString) {
      if (ch === '\\') escape = true;
      else if (ch === inString) inString = false;
      continue;
    }
    if (ch === '"' || ch === "'") { inString = ch; continue; }
    if (ch === '/' && next === '/') { inLineComment = true; i++; continue; }
    if (ch === '/' && next === '*') { inBlockComment = true; i++; continue; }
    if (ch === '{') stack.push('}');
    else if (ch === '[') stack.push(']');
    else if (ch === '}' || ch === ']') {
      if (stack.length === 0) return null;
      if (stack[stack.length - 1] !== ch) return null;
      stack.pop();
    }
  }
  if (stack.length === 0) return null;
  return content + stack.reverse().join('');
}

function tryParse(s) {
  try { JSON.parse(s); return 'OK'; }
  catch (e) { return 'FAIL — ' + e.message; }
}

const userContent = `{
  "Logging": {
    "LogLevel": {
      "Default": "Information",
      "Microsoft.AspNetCore": "Warning"
    }
}`;

console.log('Original parses?', tryParse(userContent));
const fixed = _balanceJsonBraces(userContent);
console.log('Fixed parses?  ', tryParse(fixed));
console.log('---');
console.log('Suffix added:', JSON.stringify(fixed.slice(userContent.length)));
console.log('---fixed JSON---');
console.log(fixed);
