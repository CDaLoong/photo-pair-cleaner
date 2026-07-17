import { Star } from "lucide-react";

interface RatingControlProps {
  rating: number;
  onChange: (rating: number) => void;
  disabled?: boolean;
}

export function RatingControl({
  rating,
  onChange,
  disabled = false,
}: RatingControlProps) {
  return (
    <div className="rating-control" role="group" aria-label="照片评分">
      {[1, 2, 3, 4, 5].map((value) => (
        <button
          key={value}
          type="button"
          aria-pressed={rating === value}
          aria-label={`设为 ${value} 星`}
          title={rating === value ? "再次点击清除评分" : `设为 ${value} 星`}
          className={rating >= value ? "is-active" : undefined}
          disabled={disabled}
          onClick={() => onChange(rating === value ? 0 : value)}
        >
          <Star aria-hidden="true" size={17} fill={rating >= value ? "currentColor" : "none"} />
        </button>
      ))}
    </div>
  );
}
