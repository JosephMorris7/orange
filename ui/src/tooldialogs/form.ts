import type { Mount } from './common';
import { buildForm } from './common';

export const mountForm: Mount = ({ info, body, changed }) => {
  const form = buildForm(info.params, changed);
  form.el.style.maxWidth = '560px';
  body.append(form.el);
  return {
    getParams: () => form.values(),
    validate: () => (info.id === 'protect-pdf' && !String(form.values().password ?? '').trim() ? 'Enter a password.' : null),
    cta: info.label[0] + info.label.slice(1).toLowerCase(),
  };
};
