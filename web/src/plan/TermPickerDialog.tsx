import {Dialog} from '@astryxdesign/core/Dialog';
import {Item} from '@astryxdesign/core/Item';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {useMemo} from 'react';

import {
  formatCourseCode,
  isOffTerm,
  isRiceTerm,
  shortTermLabel,
  type CourseCode,
  type Credits,
  type PlanBundle,
  type Report,
  type TermId,
} from '../domain';
import {engine} from '../engine';
import {previewLine} from './labels';
import {amberInk, greenInk} from './paint';

type TermPickerDialogProps = {
  isOpen: boolean;
  course?: CourseCode;
  credits: Credits;
  bundle: PlanBundle;
  report: Report;
  onClose: () => void;
  onPick: (term: TermId) => void;
};

/** "Add COMP 415 to…" with the same preview lines the board shows mid-drag. */
export function TermPickerDialog({
  isOpen,
  course,
  credits,
  bundle,
  report,
  onClose,
  onPick,
}: TermPickerDialogProps) {
  const previews = useMemo(() => {
    if (course === undefined) {
      return new Map<TermId, ReturnType<typeof previewLine>>();
    }
    const rows = engine.previewPlacement(
      bundle,
      report,
      course,
      undefined,
      credits,
    );
    return new Map(rows.map(r => [r.term, previewLine(r, bundle.plan)]));
  }, [bundle, report, course, credits]);

  return (
    <Dialog
      isOpen={isOpen}
      onOpenChange={open => {
        if (!open) {
          onClose();
        }
      }}
      width={420}
      padding={3}
    >
      <Stack gap={2} width="100%">
        <Text as="p" size="lg" weight="semibold">
          Add {course === undefined ? '' : formatCourseCode(course)} to…
        </Text>
        <Stack gap={0} width="100%">
          {bundle.plan.terms
            .filter(t => !isOffTerm(t.kind))
            .map(term => {
              const line = previews.get(term.id);
              const tone =
                line?.tone === 'amber'
                  ? amberInk
                  : line?.tone === 'green'
                    ? greenInk
                    : undefined;
              return (
                <Item
                  key={term.id}
                  label={shortTermLabel(term.position)}
                  density="compact"
                  layout="inline"
                  endContent={
                    <Text type="supporting" textWrap="nowrap" style={tone}>
                      {isRiceTerm(term.kind)
                        ? (line?.text ?? '')
                        : 'becomes a manual card'}
                    </Text>
                  }
                  onClick={() => {
                    onPick(term.id);
                    onClose();
                  }}
                />
              );
            })}
        </Stack>
      </Stack>
    </Dialog>
  );
}
