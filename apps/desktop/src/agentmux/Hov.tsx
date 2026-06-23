import {
  type CSSProperties,
  type DragEventHandler,
  type MouseEventHandler,
  type ReactNode,
  useState
} from "react";

type HovTag = "div" | "button" | "span";

interface HovProps {
  tag?: HovTag;
  style: CSSProperties;
  hover?: CSSProperties;
  className?: string;
  title?: string;
  ariaLabel?: string;
  draggable?: boolean;
  onClick?: MouseEventHandler<HTMLElement>;
  onContextMenu?: MouseEventHandler<HTMLElement>;
  onDragStart?: DragEventHandler<HTMLElement>;
  onDragOver?: DragEventHandler<HTMLElement>;
  onDrop?: DragEventHandler<HTMLElement>;
  onDragEnd?: DragEventHandler<HTMLElement>;
  children?: ReactNode;
  [dataAttribute: `data-${string}`]: string | number | boolean | undefined;
}

// Declarative hover wrapper standing in for the prototype's `style-hover`
// attribute: merges `hover` over `style` while the pointer is over the element.
export function Hov({
  tag = "div",
  style,
  hover,
  className,
  title,
  ariaLabel,
  draggable,
  onClick,
  onContextMenu,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
  children,
  ...dataAttributes
}: HovProps) {
  const [hovered, setHovered] = useState(false);
  const merged = hovered && hover ? { ...style, ...hover } : style;
  const common = {
    ...dataAttributes,
    style: merged,
    className: className ? `agentmux-hover ${className}` : "agentmux-hover",
    title,
    "aria-label": ariaLabel,
    draggable,
    onClick,
    onContextMenu,
    onDragStart,
    onDragOver,
    onDrop,
    onDragEnd,
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
