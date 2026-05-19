import { sanitizeLabel } from '../utils/labelUtils.js';
export const generateGanttDiagram = (diagram) => {
    const tasks = [];
    const sections = new Set();
    for (const shape of diagram.Shapes) {
        if (!shape.IsEdge && shape.Label) {
            const taskText = sanitizeLabel(shape.Label);
            const section = 'Tasks';
            sections.add(section);
            tasks.push({
                section: section,
                name: taskText,
                id: `task_${shape.Id}`,
                start: '2024-01-01',
                duration: '1d',
            });
        }
    }
    let mermaidCode = 'gantt\n';
    mermaidCode += '    title Project Timeline\n';
    mermaidCode += '    dateFormat YYYY-MM-DD\n';
    for (const section of sections) {
        mermaidCode += `    section ${section}\n`;
        const sectionTasks = tasks.filter((t) => t.section === section);
        for (const task of sectionTasks) {
            mermaidCode += `    ${task.name} :${task.id}, ${task.start}, ${task.duration}\n`;
        }
    }
    return mermaidCode;
};
//# sourceMappingURL=ganttGenerator.js.map