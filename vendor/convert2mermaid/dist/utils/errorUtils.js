export const createParseError = (message, filePath, context, lineNumber) => {
    const error = new Error(message);
    error.name = 'ParseError';
    error.filePath = filePath;
    error.context = context;
    error.lineNumber = lineNumber;
    return error;
};
export const safeJsonParse = (jsonString, filePath) => {
    try {
        return JSON.parse(jsonString);
    }
    catch (error) {
        throw createParseError(`Invalid JSON format: ${error instanceof Error ? error.message : 'Unknown error'}`, filePath, 'JSON parsing');
    }
};
export const logParseProgress = (message, filePath) => {
    const prefix = filePath ? `[${filePath}]` : '[Parser]';
    console.debug(`${prefix} ${message}`);
};
export const safeFileRead = (filePath) => {
    try {
        const fs = require('fs');
        return fs.readFileSync(filePath);
    }
    catch (error) {
        throw createParseError(`Failed to read file: ${error instanceof Error ? error.message : 'Unknown error'}`, filePath, 'File system');
    }
};
export const validateRequiredProperties = (obj, requiredProps, context) => {
    if (!obj || typeof obj !== 'object') {
        throw createParseError(`Expected object but got ${typeof obj}`, undefined, context);
    }
    const typedObj = obj;
    for (const prop of requiredProps) {
        if (!(prop in typedObj) || typedObj[prop] === undefined) {
            throw createParseError(`Missing required property: ${String(prop)}`, undefined, context);
        }
    }
    return typedObj;
};
//# sourceMappingURL=errorUtils.js.map