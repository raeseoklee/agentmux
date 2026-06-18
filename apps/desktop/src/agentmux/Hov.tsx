import { type CSSProperties, type MouseEventHandler, type ReactNode, useState } from "react";

type HovTag = "div" | "button" | "span";

interface HovProps {
  tag?: HovTag;
  style: CSSProperties;
  hover?: CSSProperties;
  className?: string;
  title?: string;
  onClick?: MouseEventHandler<HTMLElement>;
  children?: ReactNode;
}

// Declarative hover wrapper standing in for the prototype's `style-hover`
// attribute: merges `hover` over `style` while the pointer is over the element.
export function Hov({ tag = "div", style, hover, className, title, onClick, children }: HovProps) {
  const [hovered, setHovered] = useState(false);
  const merged = hovered && hover ? { ...style, ...hover } : style;
  const common = {
    style: merged,
    className: className ? `agentmux-hover ${className}` : "agentmux-hover",
    title,
    onClick,
    onMouseEnter: () => setHovered(true),
    onMouseLeave: () => setHovered(false)
  };

  if (tag === "button") {
    return (
      <button type="button" {...common}>
        {children}
      </button>
    );
  }
  if (tag === "span") {
    return <span {...common}>{children}</span>;
  }
  return <div {...common}>{children}</div>;
}
