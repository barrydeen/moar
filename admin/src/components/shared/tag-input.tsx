"use client";

import { useState } from "react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { X, Plus } from "lucide-react";
import { truncatePubkey } from "@/lib/utils/format";

interface TagInputProps {
  values: string[];
  onChange: (values: string[]) => void;
  placeholder?: string;
  validate?: (value: string) => string | null;
  truncate?: boolean;
}

export function TagInput({
  values,
  onChange,
  placeholder = "Add value...",
  validate,
  truncate = false,
}: TagInputProps) {
  const [input, setInput] = useState("");
  const [error, setError] = useState<string | null>(null);

  function handleAdd() {
    const trimmed = input.trim();
    if (!trimmed) return;

    if (validate) {
      const err = validate(trimmed);
      if (err) {
        setError(err);
        return;
      }
    }

    if (values.includes(trimmed)) {
      setError("Already added");
      return;
    }

    onChange([...values, trimmed]);
    setInput("");
    setError(null);
  }

  function handleRemove(value: string) {
    onChange(values.filter((v) => v !== value));
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAdd();
    }
  }

  return (
    <div className="space-y-2">
      <div className="flex gap-2">
        <Input
          value={input}
          onChange={(e) => {
            setInput(e.target.value);
            setError(null);
          }}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="flex-1"
        />
        <Button type="button" variant="outline" size="icon" onClick={handleAdd}>
          <Plus className="h-4 w-4" />
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
      {values.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {values.map((value) => (
            <Badge key={value} variant="secondary" className="gap-1 font-mono text-xs">
              {truncate ? truncatePubkey(value) : value}
              <button
                type="button"
                onClick={() => handleRemove(value)}
                className="ml-1 hover:text-destructive"
              >
                <X className="h-3 w-3" />
              </button>
            </Badge>
          ))}
        </div>
      )}
    </div>
  );
}
