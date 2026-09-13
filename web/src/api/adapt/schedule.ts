/**
 * Schedules, both ways: the browser reads and writes them. The only wire
 * differences are the CRN (number there, five-digit text here) inside
 * candidates and meeting owners, and the day letters inside busy blocks.
 */
import type {BusyId, ScheduleId} from '../../domain/ids';
import type {
  BusyBlock,
  Candidate,
  MeetingOwner,
  TermSchedule,
} from '../../domain/schedule';
import type {BusyBlock as WireBusyBlock} from '../generated/BusyBlock';
import type {Candidate as WireCandidate} from '../generated/Candidate';
import type {MeetingOwner as WireMeetingOwner} from '../generated/MeetingOwner';
import type {TermSchedule as WireTermSchedule} from '../generated/TermSchedule';
import {
  crnFromWire,
  crnToWire,
  meetingFromWire,
  meetingToWire,
} from './section';
import {unknownWire} from './util';

function candidateFromWire(w: WireCandidate): Candidate {
  return {
    course: w.course,
    sections: w.sections.map(crnFromWire),
    visible: w.visible,
    colour: w.colour,
  };
}

function candidateToWire(c: Candidate): WireCandidate {
  return {
    course: c.course,
    sections: c.sections.map(crnToWire),
    visible: c.visible,
    colour: c.colour,
  };
}

function busyFromWire(w: WireBusyBlock): BusyBlock {
  return {
    id: w.id as BusyId,
    label: w.label,
    meeting: meetingFromWire(w.meeting),
  };
}

function busyToWire(b: BusyBlock): WireBusyBlock {
  return {id: b.id, label: b.label, meeting: meetingToWire(b.meeting)};
}

export function meetingOwnerFromWire(w: WireMeetingOwner): MeetingOwner {
  switch (w.kind) {
    case 'section':
      return {kind: 'section', value: crnFromWire(w.value)};
    case 'busy':
      return {kind: 'busy', value: w.value as BusyId};
    default:
      return unknownWire('meetingOwner', w);
  }
}

export function meetingOwnerToWire(o: MeetingOwner): WireMeetingOwner {
  switch (o.kind) {
    case 'section':
      return {kind: 'section', value: crnToWire(o.value)};
    case 'busy':
      return {kind: 'busy', value: o.value};
    default:
      return unknownWire('meetingOwner', o);
  }
}

export function scheduleFromWire(w: WireTermSchedule): TermSchedule {
  return {
    id: w.id as ScheduleId,
    name: w.name,
    term: w.term,
    candidates: w.candidates.map(candidateFromWire),
    busy: w.busy.map(busyFromWire),
  };
}

export function scheduleToWire(s: TermSchedule): WireTermSchedule {
  return {
    id: s.id,
    name: s.name,
    term: s.term,
    candidates: s.candidates.map(candidateToWire),
    busy: s.busy.map(busyToWire),
  };
}
