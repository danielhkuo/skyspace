import type {AnchorHTMLAttributes, ReactNode} from 'react';
import {Link} from 'react-router';

type RouterLinkProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  href?: string;
  children?: ReactNode;
};

/**
 * Astryx link components take `href`; react-router's Link takes `to`.
 * In-app paths route client-side, everything else stays a plain anchor.
 */
export function RouterLink({href, children, ...rest}: RouterLinkProps) {
  if (href !== undefined && href.startsWith('/')) {
    return (
      <Link to={href} {...rest}>
        {children}
      </Link>
    );
  }
  return (
    <a href={href} {...rest}>
      {children}
    </a>
  );
}
