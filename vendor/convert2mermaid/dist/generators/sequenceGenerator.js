import { sanitizeLabel } from '../utils/labelUtils.js';
export const generateSequenceDiagram = (diagram) => {
    let mermaidSyntax = 'sequenceDiagram\r\n';
    const participants = [];
    const actors = [];
    const messages = [];
    const notes = [];
    const frames = [];
    let hasSequenceNumbers = false;
    for (const shape of diagram.Shapes) {
        if (shape.IsEdge) {
            messages.push(shape);
        }
        else if (shape.ParticipantType === 'actor') {
            actors.push(shape);
        }
        else if (shape.ParticipantType === 'participant') {
            participants.push(shape);
        }
        else if (shape.ParticipantType === 'note') {
            notes.push(shape);
        }
        else if (shape.ParticipantType === 'frame') {
            frames.push(shape);
        }
        else if (shape.ParticipantType === 'activation') {
            continue;
        }
        else if (shape.Label && /^\d+$/.test(shape.Label.trim())) {
            hasSequenceNumbers = true;
            continue;
        }
        else if (shape.Label && /^\[.*\]$/.test(shape.Label.trim())) {
            continue;
        }
        else if (!shape.Label || shape.Label.trim().length === 0) {
            continue;
        }
        else {
            participants.push(shape);
        }
    }
    const generateAlias = (label, existingAliases) => {
        if (!label || label.trim().length === 0) {
            return 'P' + Math.random().toString(36).substr(2, 4);
        }
        const words = label.split(/[\s/()]+/).filter((w) => w.length > 0);
        if (words.length === 1) {
            let alias = words[0].charAt(0).toUpperCase();
            if (existingAliases.has(alias)) {
                alias = words[0].substring(0, Math.min(3, words[0].length));
            }
            let counter = 1;
            let finalAlias = alias;
            while (existingAliases.has(finalAlias)) {
                finalAlias = alias + counter++;
            }
            return finalAlias;
        }
        else {
            let alias = words.map((w) => w.charAt(0).toUpperCase()).join('');
            if (alias.length > 4) {
                alias = alias.substring(0, 4);
            }
            let counter = 1;
            let finalAlias = alias;
            while (existingAliases.has(finalAlias)) {
                finalAlias = alias.substring(0, 3) + counter++;
            }
            return finalAlias;
        }
    };
    const aliasMap = new Map();
    const usedAliases = new Set();
    if (hasSequenceNumbers) {
        mermaidSyntax += '  autonumber\r\n\r\n';
    }
    for (const actor of actors) {
        const actorName = sanitizeLabel(actor.Label) || actor.Id;
        const alias = generateAlias(actorName, usedAliases);
        usedAliases.add(alias);
        aliasMap.set(actor.Id, alias);
        mermaidSyntax += `  actor ${alias} as ${actorName}\r\n`;
    }
    for (const participant of participants) {
        const participantName = sanitizeLabel(participant.Label) || participant.Id;
        const alias = generateAlias(participantName, usedAliases);
        usedAliases.add(alias);
        aliasMap.set(participant.Id, alias);
        mermaidSyntax += `  participant ${alias} as ${participantName}\r\n`;
    }
    for (const message of messages) {
        let from = message.FromNode || '';
        let to = message.ToNode || '';
        const messageText = sanitizeLabel(message.Label) || '';
        from = aliasMap.get(from) || from;
        to = aliasMap.get(to) || to;
        if (from && to) {
            const arrow = message.Style.LinePattern === 2 ? '-->' : '->';
            mermaidSyntax += `  ${from}${arrow}${to}: ${messageText}\r\n`;
        }
    }
    return mermaidSyntax;
};
//# sourceMappingURL=sequenceGenerator.js.map