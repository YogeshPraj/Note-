import { Command } from 'commander';
// Note++ vendor patch: ora + figlet replaced with no-op stubs because their
// transitive cli-spinners@3 dep uses the JSON import-attributes syntax
// (`import x from 'y' with {type: 'json'}`), which Electron 28's bundled
// Node 18.18.2 rejects with "Unexpected token 'with'". Both libraries
// are purely cosmetic (ASCII art header + progress spinner) — we capture
// stdout/stderr in Note++'s main process and don't render either visually.
const figlet = { textSync: () => '' };
const ora = (_text) => {
    const s = { info: () => {}, succeed: () => {}, fail: () => {}, stop: () => {} };
    s.start = () => s;
    return s;
};
import * as fs from 'fs';
import path from 'path';
import { generateMermaidCode } from './scribe.js';
import { parseData } from './parser/parser.js';
const program = new Command();
const supportedFileTypes = ['.vsdx', '.drawio', '.excalidraw', '.puml', '.plantuml'];
// figlet.textSync stub returns '' — suppress the empty console.log
// console.log(figlet.textSync('convert2mermaid'));
program
    .name('convert2mermaid')
    .version('1.0.0')
    .description('A utility to convert diagrams in other formats to MermaidJs markdown syntax')
    .requiredOption('-i, --inputFile <value>', 'Input file')
    .option('-d, --diagramType [value]', 'Type of diagram', 'flowchart')
    .option('-o, --outputFile [value]', 'Output file name - defaults to input filename')
    .option('-f, --outputFormat [value]', 'Output format', 'mmd')
    .parse(process.argv);
const options = program.opts();
const fileExt = path.extname(options.inputFile);
if (!supportedFileTypes.includes(fileExt)) {
    console.error(`Unsupported file type: ${fileExt}. Supported file types are: ${supportedFileTypes}`);
    process.exit(1);
}
processFile(options.inputFile);
async function processFile(filepath) {
    const spinner = ora(`Processing ${filepath}`).start();
    let outputFilePath = options.outputFile || filepath.replace(fileExt, '.mmd');
    let diagram = await parseData(filepath);
    if (!diagram) {
        console.error(`No diagram detected in  ${filepath}, quitting.`);
        process.exit(0);
    }
    if (diagram.Analysis) {
        spinner.info(`Detected diagram type: ${diagram.Analysis.detectedType} (${diagram.Analysis.confidence}% confidence)`);
        if (diagram.Analysis.patterns.length > 0) {
            console.log('\nDetection evidence:');
            diagram.Analysis.patterns.forEach((pattern) => {
                console.log(`  - ${pattern.type}: ${pattern.evidence.join(', ')} (${pattern.confidence}%)`);
            });
        }
        if (diagram.Analysis.metadata.totalShapes > 0) {
            console.log(`\nDiagram metadata: ${diagram.Analysis.metadata.totalShapes} shapes, ${diagram.Analysis.metadata.totalEdges} edges`);
        }
        console.log();
    }
    try {
        const mermaidSyntax = generateMermaidCode(diagram);
        fs.writeFileSync(outputFilePath, mermaidSyntax);
        spinner.succeed();
        console.log(`Mermaid syntax written to ${outputFilePath}`);
    }
    catch (error) {
        console.error('Error occurred while parsing source data!', error);
    }
    process.exit(0);
}
//# sourceMappingURL=index.js.map