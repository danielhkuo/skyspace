import {AlertDialog} from '@astryxdesign/core/AlertDialog';
import {Banner} from '@astryxdesign/core/Banner';
import {Button} from '@astryxdesign/core/Button';
import {Dialog} from '@astryxdesign/core/Dialog';
import {DropdownMenu} from '@astryxdesign/core/DropdownMenu';
import {Icon} from '@astryxdesign/core/Icon';
import {
  Layout,
  LayoutHeader,
  LayoutPanel,
  LayoutContent,
} from '@astryxdesign/core/Layout';
import {Stack} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {TextInput} from '@astryxdesign/core/TextInput';
import {useEffect, useState} from 'react';
import {useNavigate} from 'react-router';

import {dataSource} from '../datasource';
import {
  formatCredits,
  visibleCredits,
  visibleCrns,
  type TermCode,
} from '../domain';
import {LoadErrorCard} from '../shell/LoadErrorCard';
import {useSession} from '../shell/useSession';
import {DESKTOP_WIDTH, useViewportWidth} from '../shell/useViewportWidth';
import {AddCourseSearch} from './AddCourseSearch';
import {CandidateList} from './CandidateList';
import {useSchedule} from './useSchedule';
import {WeekGrid} from './WeekGrid';

const LIST_WIDTH = 340;
const LIST_WIDTH_TABLET = 300;

/** One term's schedules: candidates on the left, the week on the right. */
export function SchedulePage() {
  const [term, setTerm] = useState<{code: TermCode; label: string} | undefined>(
    undefined,
  );
  const [termFailed, setTermFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    void dataSource
      .currentTerm()
      .then(t => {
        if (live) {
          setTerm(t);
        }
      })
      .catch(() => {
        if (live) {
          setTermFailed(true);
        }
      });
    return () => {
      live = false;
    };
  }, [attempt]);

  const state = useSchedule(term?.code);
  const {session} = useSession();
  const navigate = useNavigate();
  const desktop = useViewportWidth() >= DESKTOP_WIDTH;
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [copied, setCopied] = useState(false);
  /** Set when the clipboard refused: the list is shown to copy by hand. */
  const [crnText, setCrnText] = useState<string | undefined>(undefined);

  if (termFailed || state.failed) {
    return (
      <LoadErrorCard
        what="your schedule"
        onRetry={() => {
          setTermFailed(false);
          setAttempt(n => n + 1);
          state.retry();
        }}
      />
    );
  }
  const {current, schedules} = state;
  if (term === undefined || current === undefined || schedules === undefined) {
    return (
      <Stack padding={4}>
        <Text type="supporting">Loading schedule…</Text>
      </Stack>
    );
  }

  const credits = visibleCredits(current, state.sectionsByCrn);
  const crns = visibleCrns(current);
  const copyCrns = async (): Promise<void> => {
    const text = crns.join(', ');
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // No clipboard permission (embedded browsers, some kiosks): show it instead.
      setCrnText(text);
    }
  };

  const header = (
    <Stack
      direction="horizontal"
      width="100%"
      vAlign="center"
      hAlign="between"
      gap={3}
      wrap="wrap"
      paddingInline={3}
      paddingBlock={1.5}
    >
      <Stack direction="horizontal" gap={2} vAlign="center">
        <DropdownMenu
          button={{label: current.name, variant: 'ghost', size: 'md'}}
          hasChevron
          items={[
            {
              type: 'section',
              title: term.label,
              items: schedules.map(s => ({
                id: s.id,
                label: s.name,
                description: `${s.candidates.length} course${s.candidates.length === 1 ? '' : 's'}`,
                icon:
                  s.id === current.id ? (
                    <Icon icon="check" size="sm" />
                  ) : undefined,
                onClick: () => state.switchTo(s.id),
              })),
            },
            {type: 'divider'},
            {id: 'new', label: 'New schedule', onClick: state.create},
            {
              id: 'rename',
              label: 'Rename…',
              onClick: () => setRenaming(true),
            },
            {
              id: 'delete',
              label: 'Delete…',
              variant: 'destructive',
              onClick: () => setDeleting(true),
            },
          ]}
        />
        <Text type="supporting" textWrap="nowrap">
          {term.label}
        </Text>
      </Stack>
      <Stack direction="horizontal" gap={2} vAlign="center">
        <Text weight="medium" size="sm" hasTabularNumbers textWrap="nowrap">
          {formatCredits(credits)} credit hours visible
        </Text>
        <Button
          label={copied ? 'Copied' : 'Copy CRNs'}
          variant="secondary"
          size="sm"
          icon={<Icon icon={copied ? 'check' : 'copy'} size="sm" />}
          isDisabled={crns.length === 0}
          tooltip={
            crns.length === 0
              ? 'Pick a section first'
              : 'The list to paste into Esther'
          }
          clickAction={copyCrns}
        />
        <Button
          label="Print / PDF"
          variant="secondary"
          size="sm"
          onClick={() => window.print()}
        />
      </Stack>
    </Stack>
  );

  const list = (
    <Stack width="100%" height="100%" gap={0} isScrollable>
      {session === null && (
        <Banner
          status="info"
          container="section"
          collapsible={false}
          title="Saved in this browser. Sign in to keep it on every device."
          endContent={
            <Button
              label="Sign in"
              variant="secondary"
              size="sm"
              onClick={() => void navigate('/sign-in')}
            />
          }
        />
      )}
      <Stack width="100%" gap={0} paddingInline={3}>
        {current.candidates.length === 0 && (
          <Stack width="100%" paddingBlock={3} align="start" gap={0.5}>
            <Text size="sm" weight="medium">
              No courses yet
            </Text>
            <Text type="supporting">
              Search below, or use "Add to schedule" on any section in the
              catalog.
            </Text>
          </Stack>
        )}
        <CandidateList
          schedule={current}
          sectionsByCourse={state.sectionsByCourse}
          sectionsByCrn={state.sectionsByCrn}
          conflicts={state.conflicts}
          dispatch={state.dispatch}
        />
        <AddCourseSearch
          term={term.code}
          schedule={current}
          onAdd={(course, crn) =>
            state.dispatch({type: 'addCandidate', course, crn})
          }
        />
      </Stack>
    </Stack>
  );

  return (
    <>
      <Layout
        header={
          <LayoutHeader hasDivider padding={0}>
            {header}
          </LayoutHeader>
        }
        start={
          <LayoutPanel
            width={desktop ? LIST_WIDTH : LIST_WIDTH_TABLET}
            hasDivider
            isScrollable={false}
          >
            {list}
          </LayoutPanel>
        }
        content={
          <LayoutContent padding={0} isScrollable={false}>
            <Stack width="100%" height="100%" isScrollable>
              <WeekGrid
                schedule={current}
                sectionsByCrn={state.sectionsByCrn}
                conflicts={state.conflicts}
              />
            </Stack>
          </LayoutContent>
        }
      />

      <Dialog
        isOpen={crnText !== undefined}
        onOpenChange={open => {
          if (!open) {
            setCrnText(undefined);
          }
        }}
        width={400}
        padding={3}
      >
        <Stack gap={2} width="100%">
          <Text as="p" size="lg" weight="semibold">
            CRNs to paste into Esther
          </Text>
          <TextInput
            label="CRNs"
            isLabelHidden
            value={crnText ?? ''}
            isReadOnly
            hasAutoFocus
            width="100%"
          />
          <Text type="supporting">
            This browser would not let Skyspace copy for you. Select the list
            and copy it yourself.
          </Text>
        </Stack>
      </Dialog>
      <RenameDialog
        key={`${current.id}-${renaming ? 'open' : 'closed'}`}
        isOpen={renaming}
        name={current.name}
        taken={schedules.filter(s => s.id !== current.id).map(s => s.name)}
        onClose={() => setRenaming(false)}
        onSave={name => {
          state.dispatch({type: 'rename', name});
          setRenaming(false);
        }}
      />
      <AlertDialog
        isOpen={deleting}
        onOpenChange={open => {
          if (!open) {
            setDeleting(false);
          }
        }}
        title={`Delete ${current.name}?`}
        description={
          schedules.length === 1
            ? 'Its courses go with it. An empty schedule takes its place.'
            : 'Its courses go with it; your other schedules stay.'
        }
        cancelLabel="Keep"
        actionLabel="Delete"
        actionVariant="destructive"
        onAction={() => {
          setDeleting(false);
          state.deleteCurrent();
        }}
      />
    </>
  );
}

type RenameDialogProps = {
  isOpen: boolean;
  name: string;
  taken: string[];
  onClose: () => void;
  onSave: (name: string) => void;
};

function RenameDialog({
  isOpen,
  name,
  taken,
  onClose,
  onSave,
}: RenameDialogProps) {
  const [draft, setDraft] = useState(name);
  const trimmed = draft.trim();
  const error =
    trimmed === ''
      ? 'Give it a name'
      : taken.includes(trimmed)
        ? 'Another schedule has that name'
        : undefined;
  return (
    <Dialog
      isOpen={isOpen}
      onOpenChange={open => {
        if (!open) {
          onClose();
        }
      }}
      width={400}
      padding={3}
    >
      <Stack gap={3} width="100%">
        <Text as="p" size="lg" weight="semibold">
          Rename schedule
        </Text>
        <TextInput
          label="Name"
          value={draft}
          onChange={setDraft}
          hasAutoFocus
          width="100%"
          status={
            error === undefined || draft === name
              ? undefined
              : {type: 'error', message: error}
          }
          onEnter={() => {
            if (error === undefined) {
              onSave(trimmed);
            }
          }}
        />
        <Stack direction="horizontal" gap={1} hAlign="end" width="100%">
          <Button label="Cancel" variant="ghost" size="sm" onClick={onClose} />
          <Button
            label="Save"
            variant="primary"
            size="sm"
            isDisabled={error !== undefined || trimmed === name}
            onClick={() => onSave(trimmed)}
          />
        </Stack>
      </Stack>
    </Dialog>
  );
}
