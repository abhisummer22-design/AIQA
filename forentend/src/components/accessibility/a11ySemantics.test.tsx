/**
 * Structural sweep of the accessibility semantics that a screen reader depends
 * on, and that no existing test in this suite pins.
 *
 * Three defects, one shape each: markup that is present in the DOM but
 * absent to assistive technology, so the visual design implies a relationship
 * the accessibility tree does not carry.
 *
 *   - A `<th>` with no `scope`. The table LOOKS like it has headers -- they
 *     are bold and aligned -- but with no scope a screen reader reading a cell
 *     has nothing to say which column it belongs to, so "Environment: qa",
 *     the single most useful thing about the row, is never announced.
 *   - A form control with no accessible name. A placeholder is not a label:
 *     it disappears the moment the field has content, and several screen
 *     readers do not read placeholders at all. 58 of these shipped.
 *   - A `role="tab"` with no `aria-selected`. The tab strips shipped as plain
 *     buttons, so activating one changed the panel silently and a screen
 *     reader user had no way to know which of four views they were in.
 *
 * Why a file walk instead of rendering every component: these are properties of
 * the source, not of any particular render. Rendering sixty components with
 * their provider chains and prop fixtures to assert an attribute that is either
 * present in the JSX or is not would be far more brittle and would prove less --
 * it would pass on the fixtures that happen to render the control and stay
 * silent about the states that do not. A walk of the JSX AST covers every call
 * site of every component, including the ones no fixture exercises.
 *
 * The parse uses the TypeScript compiler that already type-checks this repo, so
 * the assertions read real JSX structure rather than guessing with regular
 * expressions -- a handler containing `=>` and nested braces spans several
 * lines and defeats a naive `<input[^>]*>` match, which is how the first pass
 * at this audit both missed every self-closing `<input />` and invented a dozen
 * false positives.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
import { isTestFile, listTsxFiles } from '../../utils/listTsxFiles';

// Not `fs.globSync`: that is Node 22+, and CI is on Node 20, where all three of
// these audits failed with `TypeError: globSync is not a function`. See
// `utils/listTsxFiles` for why the walk is done by hand instead.
const SOURCES = listTsxFiles('src/components', isTestFile);

/** Tags that take a label and therefore need an accessible name. */
const FORM_CONTROLS = new Set(['input', 'select', 'textarea']);

/** One parsed source file, plus the line lookup every failure message needs. */
interface Parsed {
  file: string;
  sf: ts.SourceFile;
  lineOf(node: ts.Node): number;
}

let parsed: Parsed[] = [];

beforeAll(() => {
  parsed = SOURCES.map((file) => {
    const sf = ts.createSourceFile(file, readFileSync(file, 'utf8'), ts.ScriptTarget.ESNext, true, ts.ScriptKind.TSX);
    return {
      file,
      sf,
      lineOf: (node) => sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1,
    };
  });
});

/** `<input .../>` and `<input ...></input>` are different AST nodes. Both count. */
function isElement(node: ts.Node): node is ts.JsxElement | ts.JsxSelfClosingElement {
  return ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node);
}

function tagOf(el: ts.JsxElement | ts.JsxSelfClosingElement, sf: ts.SourceFile): string {
  return (ts.isJsxSelfClosingElement(el) ? el.tagName : el.openingElement.tagName).getText(sf);
}

function attrsOf(el: ts.JsxElement | ts.JsxSelfClosingElement) {
  return ts.isJsxSelfClosingElement(el) ? el.attributes.properties : el.openingElement.attributes.properties;
}

function attrNamed(el: ts.JsxElement | ts.JsxSelfClosingElement, name: string): ts.JsxAttribute | undefined {
  // `name` is a JsxAttributeName, which may also be namespaced (`xlink:href`);
  // only the identifier form can be compared to a plain attribute name.
  return attrsOf(el).find(
    (a): a is ts.JsxAttribute =>
      ts.isJsxAttribute(a) && ts.isIdentifier(a.name) && a.name.text === name,
  );
}

/**
 * The value of an `id`/`htmlFor`, kept as source text.
 *
 * A literal is one thing and an interpolated template is another, but
 * `htmlFor={`crawl-url-${id}`}` really does associate with
 * `id={`crawl-url-${id}`}` -- both evaluate to the same string. Comparing the
 * source text covers both without trying to evaluate anything.
 *
 * A static string comes back quoted (`"audit-limit-select"`); anything computed
 * comes back unquoted, which is how callers tell "resolvable right now" from
 * "built at runtime".
 */
function valueText(attr: ts.JsxAttribute | undefined): string | null {
  if (!attr?.initializer) return null;
  const init = attr.initializer;
  // `id="x"` and `id={"x"}` are both static.
  if (ts.isStringLiteral(init)) return `"${init.text}"`;
  if (ts.isJsxExpression(init)) {
    if (ts.isStringLiteral(init.expression)) return `"${init.expression.text}"`;
    if (ts.isNoSubstitutionTemplateLiteral(init.expression)) return `"${init.expression.text}"`;
    return init.expression.getText();
  }
  return null;
}

/** Every element in the file, depth-first. */
function elements(sf: ts.SourceFile): Array<ts.JsxElement | ts.JsxSelfClosingElement> {
  // Collected into an array rather than yielded: `ts.forEachChild` takes a void
  // callback, so a `yield` inside the walk is not inside a generator body.
  const found: Array<ts.JsxElement | ts.JsxSelfClosingElement> = [];
  const walk = (node: ts.Node): void => {
    if (isElement(node)) found.push(node);
    ts.forEachChild(node, walk);
  };
  walk(sf);
  return found;
}

// ------------------------------------------------------------------ th scope

describe('every data table header cell declares its column', () => {
  it('no <th> is missing scope', () => {
    const offenders: string[] = [];
    let checked = 0;

    for (const { file, sf, lineOf } of parsed) {
      for (const el of elements(sf)) {
        if (tagOf(el, sf) !== 'th') continue;
        checked += 1;
        // Without scope, a screen reader reading a cell cannot say which column
        // the cell belongs to -- the header text is decorative bold type.
        if (!attrNamed(el, 'scope')) {
          offenders.push(`${file}:${lineOf(el)} <th> has no scope=`);
        }
      }
    }

    // Guard against the walk silently finding nothing, which would make the
    // assertion above vacuously true and hide a broken file glob.
    expect(checked).toBeGreaterThan(0);
    expect(
      offenders,
      `these header cells look like headers but do not act like them: ${offenders.join(', ')}`,
    ).toEqual([]);
  });
});

// ------------------------------------------------------- control accessible names

describe('every form control has an accessible name', () => {
  it('no input, select or textarea is nameless', () => {
    const offenders: string[] = [];
    let checked = 0;

    for (const { file, sf, lineOf } of parsed) {
      // What this file's <label htmlFor> values resolve to, so an association
      // is judged by the file that actually contains both halves.
      const htmlForLiterals = new Set<string>();
      const htmlForExprs = new Set<string>();
      for (const el of elements(sf)) {
        if (tagOf(el, sf) !== 'label') continue;
        const v = valueText(attrNamed(el, 'htmlFor'));
        if (v === null) continue;
        if (v.startsWith('"')) htmlForLiterals.add(v);
        else htmlForExprs.add(v);
      }

      /** Depth of enclosing <label> elements -- a wrapping label is a real name. */
      const labelDepth = (node: ts.Node): number => {
        let depth = 0;
        let p: ts.Node | undefined = node.parent;
        while (p) {
          if (ts.isJsxElement(p) && p.openingElement.tagName.getText(sf) === 'label') depth += 1;
          p = p.parent;
        }
        return depth;
      };

      for (const el of elements(sf)) {
        if (!FORM_CONTROLS.has(tagOf(el, sf))) continue;
        checked += 1;

        if (attrNamed(el, 'aria-label') ?? attrNamed(el, 'aria-labelledby')) continue;

        const id = valueText(attrNamed(el, 'id'));
        const associatedByFor =
          id !== null &&
          (id.startsWith('"') ? htmlForLiterals.has(id) : htmlForExprs.has(id));

        if (associatedByFor) continue;
        if (labelDepth(el) > 0) continue; // wrapped by a <label>

        offenders.push(`${file}:${lineOf(el)} <${tagOf(el, sf)}> has no accessible name`);
      }
    }

    expect(checked).toBeGreaterThan(0);
    expect(
      offenders,
      `these controls are announced with no name at all, so a screen reader ` +
        `user hears only "edit text" or "combo box": ${offenders.join(', ')}`,
    ).toEqual([]);
  });
});

// --------------------------------------------------------------- tab semantics

describe('every tab declares whether it is the selected one', () => {
  it('no role="tab" is missing aria-selected', () => {
    const offenders: string[] = [];
    let checked = 0;

    for (const { file, sf, lineOf } of parsed) {
      for (const el of elements(sf)) {
        if (attrNamed(el, 'role')?.initializer === undefined) continue;
        const role = valueText(attrNamed(el, 'role'));
        if (role !== '"tab"') continue;
        checked += 1;
        // A tab without aria-selected is a button that swaps a panel silently:
        // nothing tells a screen reader which of the views is now showing.
        if (!attrNamed(el, 'aria-selected')) {
          offenders.push(`${file}:${lineOf(el)} role="tab" has no aria-selected`);
        }
      }
    }

    // The tab strips exist (three of them), so a count of zero here means the
    // sweep is broken rather than that the defect is absent.
    expect(checked).toBeGreaterThan(0);
    expect(
      offenders,
      `these tabs change a panel without saying so: ${offenders.join(', ')}`,
    ).toEqual([]);
  });

  it('each tab names the panel it controls, and each panel names its tab', () => {
    const offenders: string[] = [];

    for (const { file, sf, lineOf } of parsed) {
      for (const el of elements(sf)) {
        const role = valueText(attrNamed(el, 'role'));
        if (role !== '"tab"' && role !== '"tabpanel"') continue;

        // The tab owns the panel, so a tab must say which panel it controls and
        // a panel must say which tab it belongs to; `aria-labelledby` is what
        // gives the panel its accessible name. Both halves are required, and
        // both need an id for the other half to point at.
        const partner = role === '"tab"' ? 'aria-controls' : 'aria-labelledby';
        if (valueText(attrNamed(el, partner)) === null) {
          offenders.push(`${file}:${lineOf(el)} role=${role} has no ${partner}`);
        }
        if (valueText(attrNamed(el, 'id')) === null) {
          offenders.push(`${file}:${lineOf(el)} role=${role} has no id for its partner to point at`);
        }
      }
    }

    // Whether the two halves actually match cannot be settled statically: both
    // strips generate their tab ids from a `tabs` array (`id={`tab-${tab.id}`}`),
    // so the id exists only once the component runs. Asserting the wiring here
    // and leaving the resolution to a render test is the honest split.
    expect(
      offenders,
      `a tab and its panel are only connected for assistive tech if each names ` +
        `the other: ${offenders.join(', ')}`,
    ).toEqual([]);
  });
});
