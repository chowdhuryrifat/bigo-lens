import type { ConstraintFormState } from '../services/constraintsFormModel';

export interface FormReadyMessage {
  type: 'ready';
}

export interface FormValidateMessage {
  type: 'validate';
  state: ConstraintFormState;
}

export interface FormSaveMessage {
  type: 'save';
  state: ConstraintFormState;
  andAnalyze?: boolean;
}

export type ConstraintFormIncomingMessage = FormReadyMessage | FormValidateMessage | FormSaveMessage;

export type FormMessageValidationResult =
  { ok: true; message: ConstraintFormIncomingMessage } | { ok: false; error: string };

function isValidFormStateObject(rawState: unknown): rawState is ConstraintFormState {
  if (typeof rawState !== 'object' || rawState === null || Array.isArray(rawState)) {
    return false;
  }
  const s = rawState as Record<string, unknown>;
  return (
    Array.isArray(s.variables) &&
    Array.isArray(s.sumConstraints) &&
    Array.isArray(s.productConstraints) &&
    Array.isArray(s.linearConstraints)
  );
}

/**
 * Strict validator for messages sent from the constraint form webview.
 * Rejects unknown message types, non-objects, and invalid form states.
 */
export function validateConstraintFormMessage(raw: unknown): FormMessageValidationResult {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    return { ok: false, error: 'Message must be a non-null object' };
  }

  const candidate = raw as Record<string, unknown>;
  if (typeof candidate.type !== 'string' || candidate.type.trim() === '') {
    return { ok: false, error: 'Message must contain a non-empty string "type" property' };
  }

  switch (candidate.type) {
    case 'ready': {
      return { ok: true, message: { type: 'ready' } };
    }
    case 'validate': {
      if (!isValidFormStateObject(candidate.state)) {
        return { ok: false, error: 'Validate message requires a valid "state" object' };
      }
      return {
        ok: true,
        message: {
          type: 'validate',
          state: candidate.state,
        },
      };
    }
    case 'save': {
      if (!isValidFormStateObject(candidate.state)) {
        return { ok: false, error: 'Save message requires a valid "state" object' };
      }
      const andAnalyze = candidate.andAnalyze === true;
      return {
        ok: true,
        message: {
          type: 'save',
          state: candidate.state,
          andAnalyze,
        },
      };
    }
    default: {
      return { ok: false, error: `Unknown form message type: "${candidate.type}"` };
    }
  }
}
