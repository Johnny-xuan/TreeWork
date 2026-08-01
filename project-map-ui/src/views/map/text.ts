export function truncate(value: string, maximum: number): string {
  if (value.length <= maximum) {
    return value;
  }
  return `${value.slice(0, Math.max(1, maximum - 1)).trimEnd()}…`;
}

export function wrapPurpose(value: string, lineLength = 34): string[] {
  const words = value.trim().split(/\s+/).filter(Boolean);
  if (!words.length) {
    return [];
  }
  const lines: string[] = [];
  let line = "";
  for (const word of words) {
    const candidate = line ? `${line} ${word}` : word;
    if (candidate.length <= lineLength || !line) {
      line = candidate;
      continue;
    }
    lines.push(line);
    line = word;
    if (lines.length === 2) {
      break;
    }
  }
  if (lines.length < 2 && line) {
    lines.push(line);
  }
  const consumed = lines.join(" ").length;
  if (consumed < value.trim().length && lines.length) {
    lines[lines.length - 1] = truncate(lines[lines.length - 1], lineLength);
  }
  return lines.slice(0, 2);
}
